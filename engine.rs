use chrono::Local;
use encoding_rs::{Encoding, BIG5, GB18030, UTF_16BE, UTF_16LE, UTF_8};
use regex_lite::RegexBuilder;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::Mutex;

use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, CW_USEDEFAULT, MB_ICONERROR, MB_OK};

pub const LARGE_FILE_BYTES: usize = 20 * 1024 * 1024; // >20MB 触发大文件自动降级策略
pub const MAX_MATCH_HIGHLIGHTS: usize = 50_000;

pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn from_wide_slice(slice: &[u16]) -> String {
    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..end])
}

// ============================================================================
// 1. 启动期错误日志与崩溃捕获（程序入口第一行即启用）
// ============================================================================

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn init_logger() {
    let base = exe_dir();
    let candidate = base.join("app.log");
    let chosen = if OpenOptions::new().create(true).append(true).open(&candidate).is_ok() {
        candidate
    } else {
        std::env::temp_dir().join("NotepadLite_startup.log")
    };
    if let Ok(mut guard) = LOG_PATH.lock() {
        *guard = Some(chosen);
    }
    log_msg("INFO", "=== NotepadLite starting ===");

    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let msg = format!("PANIC at [{}]: {}", loc, info);
        log_msg("FATAL", &msg);
        show_fatal_box(
            "NotepadLite 启动/运行致命错误",
            &format!("{}\n\n详细信息已写入同级目录 app.log", msg),
        );
    }));
}

pub fn log_msg(level: &str, msg: &str) {
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{}] [{}] {}\r\n", ts, level, msg);
    if let Ok(guard) = LOG_PATH.lock() {
        if let Some(path) = guard.as_ref() {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
}

pub fn show_fatal_box(title: &str, msg: &str) {
    let wt = to_wide(title);
    let wm = to_wide(msg);
    unsafe {
        MessageBoxW(null_mut(), wm.as_ptr(), wt.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

// ============================================================================
// 2. 绿色便携配置（严格保存在 exe 同级目录 config.json）
// ============================================================================

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub win_x: i32,
    pub win_y: i32,
    pub win_w: i32,
    pub win_h: i32,
    pub maximized: bool,
    pub font_name: String,
    pub font_size: i32,
    pub zoom_pct: i32,
    pub dark_theme: bool,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub show_whitespace: bool,
    pub always_on_top: bool,
    pub recent_files: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            win_x: CW_USEDEFAULT,
            win_y: CW_USEDEFAULT,
            win_w: 1080,
            win_h: 720,
            maximized: false,
            font_name: "Consolas".to_string(),
            font_size: 15,
            zoom_pct: 100,
            dark_theme: true,
            word_wrap: false,
            show_line_numbers: true,
            show_whitespace: false,
            always_on_top: false,
            recent_files: Vec::new(),
        }
    }
}

fn config_path() -> PathBuf {
    exe_dir().join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if !path.exists() {
        let cfg = AppConfig::default();
        save_config(&cfg);
        return cfg;
    }
    match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str::<AppConfig>(&s).unwrap_or_else(|e| {
            log_msg("WARN", &format!("config.json parse error, using default: {}", e));
            AppConfig::default()
        }),
        Err(e) => {
            log_msg("WARN", &format!("config.json read error: {}", e));
            AppConfig::default()
        }
    }
}

pub fn save_config(cfg: &AppConfig) {
    let path = config_path();
    match serde_json::to_string_pretty(cfg) {
        Ok(json) => {
            if let Err(e) = fs::write(&path, json) {
                log_msg("ERROR", &format!("save config.json failed: {}", e));
            }
        }
        Err(e) => log_msg("ERROR", &format!("serialize config failed: {}", e)),
    }
}

// ============================================================================
// 3. 字符编码识别与转换（UTF-8 / UTF-8 BOM / UTF-16 LE&BE / GBK / Big5）
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DocEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Gbk,
    Big5,
}

impl DocEncoding {
    pub fn label(self) -> &'static str {
        match self {
            DocEncoding::Utf8 => "UTF-8",
            DocEncoding::Utf8Bom => "UTF-8 BOM",
            DocEncoding::Utf16Le => "UTF-16 LE",
            DocEncoding::Utf16Be => "UTF-16 BE",
            DocEncoding::Gbk => "GB18030 / GBK",
            DocEncoding::Big5 => "Big5",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Crlf,
    Lf,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Crlf => "CRLF",
            LineEnding::Lf => "LF",
        }
    }
}

pub fn normalize_newlines_fast(input: &str) -> (String, LineEnding) {
    let bytes = input.as_bytes();
    if !bytes.contains(&b'\r') {
        return (input.to_string(), LineEnding::Lf);
    }
    let eol = if input.contains("\r\n") {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    };
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                out.push(b'\n');
                i += 2;
                continue;
            }
            out.push(b'\n');
            i += 1;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let s = unsafe { String::from_utf8_unchecked(out) };
    (s, eol)
}

pub fn load_file_with_encoding(
    path: &Path,
    forced: Option<DocEncoding>,
) -> Result<(String, DocEncoding, LineEnding, usize), String> {
    let bytes = fs::read(path).map_err(|e| format!("读取文件失败: {}", e))?;
    let byte_size = bytes.len();

    let (doc_enc, bom_len, rs_enc): (DocEncoding, usize, &'static Encoding) = if let Some(fe) = forced {
        let (bl, re) = match fe {
            DocEncoding::Utf8 => (0, UTF_8),
            DocEncoding::Utf8Bom => (if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 }, UTF_8),
            DocEncoding::Utf16Le => (if bytes.starts_with(&[0xFF, 0xFE]) { 2 } else { 0 }, UTF_16LE),
            DocEncoding::Utf16Be => (if bytes.starts_with(&[0xFE, 0xFF]) { 2 } else { 0 }, UTF_16BE),
            DocEncoding::Gbk => (0, GB18030),
            DocEncoding::Big5 => (0, BIG5),
        };
        (fe, bl, re)
    } else if let Some((enc, bl)) = Encoding::for_bom(&bytes) {
        let de = if enc == UTF_8 {
            DocEncoding::Utf8Bom
        } else if enc == UTF_16LE {
            DocEncoding::Utf16Le
        } else if enc == UTF_16BE {
            DocEncoding::Utf16Be
        } else {
            DocEncoding::Utf8
        };
        (de, bl, enc)
    } else if std::str::from_utf8(&bytes).is_ok() {
        (DocEncoding::Utf8, 0, UTF_8)
    } else {
        let mut detector = chardetng::EncodingDetector::new();
        let sample = bytes.len().min(256 * 1024);
        detector.feed(&bytes[..sample], bytes.len() <= sample);
        let guessed = detector.guess(None, true);
        let de = if guessed == BIG5 {
            DocEncoding::Big5
        } else if guessed == UTF_16LE {
            DocEncoding::Utf16Le
        } else if guessed == UTF_16BE {
            DocEncoding::Utf16Be
        } else if guessed == UTF_8 {
            DocEncoding::Utf8
        } else {
            DocEncoding::Gbk
        };
        (de, 0, guessed)
    };

    let (decoded, _, _) = rs_enc.decode(&bytes[bom_len..]);
    let (normalized, eol) = normalize_newlines_fast(&decoded);
    Ok((normalized, doc_enc, eol, byte_size))
}

pub fn save_file_with_encoding(
    path: &Path,
    text: &str,
    enc: DocEncoding,
    eol: LineEnding,
) -> Result<usize, String> {
    let mut prepared = String::new();
    let target_str: &str = if eol == LineEnding::Crlf {
        prepared.reserve(text.len() + text.len() / 16);
        for b in text.as_bytes() {
            if *b == b'\n' {
                prepared.push_str("\r\n");
            } else {
                unsafe { prepared.as_mut_vec().push(*b) };
            }
        }
        &prepared
    } else {
        text
    };

    let out_bytes: Vec<u8> = match enc {
        DocEncoding::Utf8 => target_str.as_bytes().to_vec(),
        DocEncoding::Utf8Bom => {
            let mut v = Vec::with_capacity(3 + target_str.len());
            v.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            v.extend_from_slice(target_str.as_bytes());
            v
        }
        DocEncoding::Utf16Le => {
            let mut v = Vec::with_capacity(2 + target_str.len() * 2);
            v.extend_from_slice(&[0xFF, 0xFE]);
            for u in target_str.encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            v
        }
        DocEncoding::Utf16Be => {
            let mut v = Vec::with_capacity(2 + target_str.len() * 2);
            v.extend_from_slice(&[0xFE, 0xFF]);
            for u in target_str.encode_utf16() {
                v.extend_from_slice(&u.to_be_bytes());
            }
            v
        }
        DocEncoding::Gbk => {
            let (cow, _, _) = GB18030.encode(target_str);
            cow.into_owned()
        }
        DocEncoding::Big5 => {
            let (cow, _, _) = BIG5.encode(target_str);
            cow.into_owned()
        }
    };

    let len = out_bytes.len();
    fs::write(path, &out_bytes).map_err(|e| format!("保存文件失败: {}", e))?;
    Ok(len)
}

// ============================================================================
// 4. 大文件高性能文本缓冲、增量行索引、多光标与撤销重做引擎
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub anchor: usize,
    pub head: usize,
}

impl Cursor {
    pub fn collapsed(pos: usize) -> Self {
        Self { anchor: pos, head: pos }
    }
    pub fn range(self) -> (usize, usize) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
    pub fn has_selection(self) -> bool {
        self.anchor != self.head
    }
}

#[derive(Clone)]
struct SingleUndoOp {
    pos: usize,
    deleted: String,
    inserted: String,
}

#[derive(Clone)]
struct UndoEntry {
    ops: Vec<SingleUndoOp>,
    cursors_before: Vec<Cursor>,
    cursors_after: Vec<Cursor>,
}

#[inline]
fn clamp_str_boundary(s: &str, pos: usize) -> usize {
    let mut p = pos.min(s.len());
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

pub struct Document {
    pub text: String,
    pub line_starts: Vec<usize>,
    pub cursors: Vec<Cursor>,
    undo_stack: VecDeque<UndoEntry>,
    redo_stack: VecDeque<UndoEntry>,
    pub dirty: bool,
    pub path: Option<PathBuf>,
    pub encoding: DocEncoding,
    pub line_ending: LineEnding,
    pub disk_bytes: usize,
    pub large_mode: bool,
    pub total_chars: usize,
}

impl Document {
    pub fn new_empty() -> Self {
        Self {
            text: String::new(),
            line_starts: vec![0],
            cursors: vec![Cursor::collapsed(0)],
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            dirty: false,
            path: None,
            encoding: DocEncoding::Utf8,
            line_ending: LineEnding::Crlf,
            disk_bytes: 0,
            large_mode: false,
            total_chars: 0,
        }
    }

    pub fn rebuild_line_index(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);
        for (i, b) in self.text.bytes().enumerate() {
            if b == b'\n' {
                self.line_starts.push(i + 1);
            }
        }
        self.total_chars = if self.text.len() > LARGE_FILE_BYTES {
            self.text.len()
        } else {
            self.text.chars().count()
        };
    }

    pub fn set_content(
        &mut self,
        text: String,
        path: Option<PathBuf>,
        enc: DocEncoding,
        eol: LineEnding,
        disk_bytes: usize,
    ) {
        self.large_mode = text.len() > LARGE_FILE_BYTES;
        self.text = text;
        self.rebuild_line_index();
        self.cursors = vec![Cursor::collapsed(0)];
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.dirty = false;
        self.path = path;
        self.encoding = enc;
        self.line_ending = eol;
        self.disk_bytes = disk_bytes;
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line_of_byte(&self, pos: usize) -> usize {
        let p = pos.min(self.text.len());
        self.line_starts.partition_point(|&s| s <= p).saturating_sub(1)
    }

    pub fn line_bounds(&self, line: usize) -> (usize, usize) {
        if line >= self.line_starts.len() {
            let len = self.text.len();
            return (len, len);
        }
        let start = self.line_starts[line];
        let end = if line + 1 < self.line_starts.len() {
            self.line_starts[line + 1].saturating_sub(1)
        } else {
            self.text.len()
        };
        (start, end.max(start))
    }

    pub fn line_str(&self, line: usize) -> &str {
        let (s, e) = self.line_bounds(line);
        &self.text[s..e]
    }

    pub fn byte_to_line_col(&self, pos: usize) -> (usize, usize) {
        let p = self.clamp_char_boundary(pos);
        let line = self.line_of_byte(p);
        let start = self.line_starts[line];
        let col = self.text[start..p].chars().count();
        (line, col)
    }

    pub fn line_col_to_byte(&self, line: usize, char_col: usize) -> usize {
        let l = line.min(self.line_count().saturating_sub(1));
        let (start, end) = self.line_bounds(l);
        let slice = &self.text[start..end];
        if let Some((idx, _)) = slice.char_indices().nth(char_col) {
            start + idx
        } else {
            end
        }
    }

    pub fn clamp_char_boundary(&self, pos: usize) -> usize {
        clamp_str_boundary(&self.text, pos)
    }

    pub fn prev_char_boundary(&self, pos: usize) -> usize {
        let mut p = self.clamp_char_boundary(pos);
        if p == 0 {
            return 0;
        }
        p -= 1;
        while p > 0 && !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    pub fn next_char_boundary(&self, pos: usize) -> usize {
        let p = self.clamp_char_boundary(pos);
        if p >= self.text.len() {
            return self.text.len();
        }
        let mut n = p + 1;
        while n < self.text.len() && !self.text.is_char_boundary(n) {
            n += 1;
        }
        n
    }

    pub fn primary(&self) -> Cursor {
        self.cursors[0]
    }

    pub fn normalize_cursors(&mut self) {
        if self.cursors.is_empty() {
            self.cursors.push(Cursor::collapsed(0));
            return;
        }
        for c in &mut self.cursors {
            c.anchor = clamp_str_boundary(&self.text, c.anchor);
            c.head = clamp_str_boundary(&self.text, c.head);
        }
        if self.cursors.len() > 1 {
            self.cursors.sort_by_key(|c| c.range());
            let mut merged: Vec<Cursor> = Vec::with_capacity(self.cursors.len());
            for c in self.cursors.drain(..) {
                if let Some(last) = merged.last_mut() {
                    let (ls, le) = last.range();
                    let (cs, ce) = c.range();
                    if cs <= le
                        && (cs == ce && ls == le
                            || cs < le
                            || (cs == le && !c.has_selection() && !last.has_selection()))
                    {
                        let new_s = ls.min(cs);
                        let new_e = le.max(ce);
                        *last = Cursor { anchor: new_s, head: new_e };
                        continue;
                    }
                }
                merged.push(c);
            }
            self.cursors = merged;
        }
    }

    fn replace_range_incremental(&mut self, start: usize, end: usize, rep: &str) {
        let start_line = self.line_of_byte(start);
        let end_line = self.line_of_byte(end);
        let delta = rep.len() as isize - (end - start) as isize;

        if !self.large_mode {
            let del_chars = self.text[start..end].chars().count();
            let ins_chars = rep.chars().count();
            self.total_chars = self.total_chars - del_chars + ins_chars;
        } else {
            self.total_chars = (self.total_chars as isize + delta).max(0) as usize;
        }

        self.text.replace_range(start..end, rep);

        let mut new_lines = Vec::new();
        for (i, b) in rep.bytes().enumerate() {
            if b == b'\n' {
                new_lines.push(start + i + 1);
            }
        }
        for s in &mut self.line_starts[end_line + 1..] {
            *s = (*s as isize + delta) as usize;
        }
        self.line_starts.splice((start_line + 1)..=end_line, new_lines);
        self.large_mode = self.text.len() > LARGE_FILE_BYTES;
    }

    pub fn apply_edits(&mut self, mut edits: Vec<(usize, usize, String)>, new_cursors: Option<Vec<Cursor>>) {
        if edits.is_empty() {
            return;
        }
        edits.sort_by(|a, b| b.0.cmp(&a.0));
        let cursors_before = self.cursors.clone();
        let mut undo_ops = Vec::with_capacity(edits.len());

        for (start, end, rep) in edits {
            let s = self.clamp_char_boundary(start);
            let e = self.clamp_char_boundary(end).max(s);
            let deleted = self.text[s..e].to_string();
            if deleted == rep {
                continue;
            }
            self.replace_range_incremental(s, e, &rep);
            undo_ops.push(SingleUndoOp {
                pos: s,
                deleted,
                inserted: rep,
            });
        }

        if undo_ops.is_empty() {
            return;
        }

        if let Some(nc) = new_cursors {
            self.cursors = nc;
        } else {
            let mut next_cursors = Vec::new();
            let mut shift: isize = 0;
            let mut asc_ops: Vec<&SingleUndoOp> = undo_ops.iter().rev().collect();
            for op in asc_ops.drain(..) {
                let p = (op.pos as isize + shift) as usize + op.inserted.len();
                next_cursors.push(Cursor::collapsed(p));
                shift += op.inserted.len() as isize - op.deleted.len() as isize;
            }
            self.cursors = next_cursors;
        }
        self.normalize_cursors();

        self.undo_stack.push_back(UndoEntry {
            ops: undo_ops,
            cursors_before,
            cursors_after: self.cursors.clone(),
        });
        if self.undo_stack.len() > 200 {
            self.undo_stack.pop_front();
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop_back() else {
            return false;
        };
        for op in entry.ops.iter().rev() {
            let end = op.pos + op.inserted.len();
            self.replace_range_incremental(op.pos, end, &op.deleted);
        }
        self.cursors = entry.cursors_before.clone();
        self.normalize_cursors();
        self.redo_stack.push_back(entry);
        self.dirty = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo_stack.pop_back() else {
            return false;
        };
        for op in &entry.ops {
            let end = op.pos + op.deleted.len();
            self.replace_range_incremental(op.pos, end, &op.inserted);
        }
        self.cursors = entry.cursors_after.clone();
        self.normalize_cursors();
        self.undo_stack.push_back(entry);
        self.dirty = true;
        true
    }

    pub fn insert_text(&mut self, input: &str) {
        let mut edits = Vec::with_capacity(self.cursors.len());
        for c in &self.cursors {
            let (s, e) = c.range();
            edits.push((s, e, input.to_string()));
        }
        self.apply_edits(edits, None);
    }

    pub fn backspace(&mut self) {
        let mut edits = Vec::new();
        for c in &self.cursors {
            let (s, e) = c.range();
            if s < e {
                edits.push((s, e, String::new()));
            } else if s > 0 {
                let prev = self.prev_char_boundary(s);
                edits.push((prev, s, String::new()));
            }
        }
        self.apply_edits(edits, None);
    }

    pub fn delete_forward(&mut self) {
        let mut edits = Vec::new();
        for c in &self.cursors {
            let (s, e) = c.range();
            if s < e {
                edits.push((s, e, String::new()));
            } else if e < self.text.len() {
                let next = self.next_char_boundary(e);
                edits.push((e, next, String::new()));
            }
        }
        self.apply_edits(edits, None);
    }

    pub fn selected_text_joined(&self) -> String {
        let mut parts = Vec::new();
        for c in &self.cursors {
            let (s, e) = c.range();
            if s < e {
                parts.push(&self.text[s..e]);
            }
        }
        parts.join("\r\n")
    }

    pub fn selected_char_count(&self) -> usize {
        self.cursors
            .iter()
            .map(|c| {
                let (s, e) = c.range();
                if s < e && (e - s) < 1_000_000 {
                    self.text[s..e].chars().count()
                } else {
                    e - s
                }
            })
            .sum()
    }

    pub fn duplicate_current_line(&mut self) {
        let c = self.primary();
        let (s, e) = c.range();
        let l1 = self.line_of_byte(s);
        let l2 = self.line_of_byte(e);
        let (b_start, _) = self.line_bounds(l1);
        let (_, b_end) = self.line_bounds(l2);
        let block = self.text[b_start..b_end].to_string();
        let ins = format!("\n{}", block);
        let new_cur = Cursor::collapsed(b_end + 1 + (c.head - b_start));
        self.apply_edits(vec![(b_end, b_end, ins)], Some(vec![new_cur]));
    }

    pub fn delete_current_line(&mut self) {
        let c = self.primary();
        let (s, e) = c.range();
        let l1 = self.line_of_byte(s);
        let l2 = self.line_of_byte(e);
        let (b_start, _) = self.line_bounds(l1);
        let (_, b_end) = self.line_bounds(l2);
        let (del_s, del_e) = if b_end < self.text.len() {
            (b_start, b_end + 1)
        } else if b_start > 0 {
            (b_start - 1, b_end)
        } else {
            (0, b_end)
        };
        let target = del_s.min(self.text.len());
        self.apply_edits(vec![(del_s, del_e, String::new())], Some(vec![Cursor::collapsed(target)]));
    }

    pub fn move_lines(&mut self, up: bool) {
        let c = self.primary();
        let (s, e) = c.range();
        let l1 = self.line_of_byte(s);
        let l2 = self.line_of_byte(e);
        if up {
            if l1 == 0 {
                return;
            }
            let (prev_s, prev_e) = self.line_bounds(l1 - 1);
            let (blk_s, _) = self.line_bounds(l1);
            let (_, blk_e) = self.line_bounds(l2);
            let prev_txt = self.text[prev_s..prev_e].to_string();
            let blk_txt = self.text[blk_s..blk_e].to_string();
            let combined = format!("{}\n{}", blk_txt, prev_txt);
            let shift = (prev_e - prev_s + 1) as isize;
            let nc = Cursor {
                anchor: (c.anchor as isize - shift) as usize,
                head: (c.head as isize - shift) as usize,
            };
            self.apply_edits(vec![(prev_s, blk_e, combined)], Some(vec![nc]));
        } else {
            if l2 + 1 >= self.line_count() {
                return;
            }
            let (blk_s, _) = self.line_bounds(l1);
            let (_, blk_e) = self.line_bounds(l2);
            let (next_s, next_e) = self.line_bounds(l2 + 1);
            let blk_txt = self.text[blk_s..blk_e].to_string();
            let next_txt = self.text[next_s..next_e].to_string();
            let combined = format!("{}\n{}", next_txt, blk_txt);
            let shift = (next_e - next_s + 1) as isize;
            let nc = Cursor {
                anchor: (c.anchor as isize + shift) as usize,
                head: (c.head as isize + shift) as usize,
            };
            self.apply_edits(vec![(blk_s, next_e, combined)], Some(vec![nc]));
        }
    }

    pub fn toggle_comment(&mut self, lang: Lang) {
        let prefix = match lang {
            Lang::Python | Lang::Shell | Lang::Ini => "# ",
            Lang::Plain | Lang::Markdown | Lang::Json | Lang::XmlHtml | Lang::Cpp | Lang::JsTs => "// ",
        };
        let prefix_trim = prefix.trim_end();
        let c = self.primary();
        let (s, e) = c.range();
        let l1 = self.line_of_byte(s);
        let l2 = self.line_of_byte(e);

        let all_commented = (l1..=l2).all(|l| {
            let t = self.line_str(l).trim_start();
            t.is_empty() || t.starts_with(prefix_trim)
        });

        let mut edits = Vec::new();
        for l in l1..=l2 {
            let (ls, _) = self.line_bounds(l);
            let line = self.line_str(l);
            let ws_len = line.len() - line.trim_start().len();
            if all_commented {
                let rest = &line[ws_len..];
                if rest.starts_with(prefix) {
                    edits.push((ls + ws_len, ls + ws_len + prefix.len(), String::new()));
                } else if rest.starts_with(prefix_trim) {
                    edits.push((ls + ws_len, ls + ws_len + prefix_trim.len(), String::new()));
                }
            } else if !line.trim().is_empty() {
                edits.push((ls + ws_len, ls + ws_len, prefix.to_string()));
            }
        }
        self.apply_edits(edits, None);
    }

    pub fn transform_case(&mut self, upper: bool) {
        let mut edits = Vec::new();
        let mut new_cursors = Vec::new();
        for c in &self.cursors {
            let (s, e) = c.range();
            if s < e {
                let sub = &self.text[s..e];
                let converted = if upper { sub.to_uppercase() } else { sub.to_lowercase() };
                let clen = converted.len();
                edits.push((s, e, converted));
                new_cursors.push(Cursor { anchor: s, head: s + clen });
            }
        }
        if !edits.is_empty() {
            self.apply_edits(edits, Some(new_cursors));
        }
    }
}

// ============================================================================
// 5. 查找与正则替换引擎
// ============================================================================

pub struct SearchState {
    pub visible: bool,
    pub show_replace: bool,
    pub query: String,
    pub replace_with: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    pub matches: Vec<(usize, usize)>,
    pub current_idx: Option<usize>,
    pub last_error: Option<String>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            visible: false,
            show_replace: false,
            query: String::new(),
            replace_with: String::new(),
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            matches: Vec::new(),
            current_idx: None,
            last_error: None,
        }
    }

    pub fn recompute(&mut self, text: &str, caret: usize) {
        self.matches.clear();
        self.current_idx = None;
        self.last_error = None;
        if self.query.is_empty() {
            return;
        }
        let mut pattern = if self.use_regex {
            self.query.clone()
        } else {
            regex_lite::escape(&self.query)
        };
        if self.whole_word {
            pattern = format!(r"\b(?:{})\b", pattern);
        }
        let re = match RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                self.last_error = Some(e.to_string());
                return;
            }
        };

        for m in re.find_iter(text) {
            if m.start() == m.end() {
                continue;
            }
            self.matches.push((m.start(), m.end()));
            if self.matches.len() >= MAX_MATCH_HIGHLIGHTS {
                break;
            }
        }
        if !self.matches.is_empty() {
            let idx = self.matches.iter().position(|&(s, _)| s >= caret).unwrap_or(0);
            self.current_idx = Some(idx);
        }
    }

    pub fn step(&mut self, forward: bool) -> Option<(usize, usize)> {
        if self.matches.is_empty() {
            return None;
        }
        let len = self.matches.len();
        let next = match self.current_idx {
            Some(i) => {
                if forward {
                    (i + 1) % len
                } else {
                    (i + len - 1) % len
                }
            }
            None => 0,
        };
        self.current_idx = Some(next);
        Some(self.matches[next])
    }
}

// ============================================================================
// 6. 轻量级可见行即时语法高亮器（O(屏幕可见行数)，大文件零卡顿）
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Plain,
    Markdown,
    Json,
    XmlHtml,
    Python,
    Cpp,
    JsTs,
    Shell,
    Ini,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    StringLit,
    Comment,
    Number,
    Accent,
}

pub fn detect_lang(path: Option<&Path>) -> Lang {
    let ext = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" => Lang::Markdown,
        "json" | "jsonc" => Lang::Json,
        "xml" | "html" | "htm" | "svg" => Lang::XmlHtml,
        "py" | "pyw" => Lang::Python,
        "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" | "rs" => Lang::Cpp,
        "js" | "jsx" | "ts" | "tsx" | "mjs" => Lang::JsTs,
        "sh" | "bash" | "bat" | "cmd" | "ps1" => Lang::Shell,
        "ini" | "toml" | "cfg" | "conf" => Lang::Ini,
        _ => Lang::Plain,
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_keyword(lang: Lang, word: &str) -> bool {
    match lang {
        Lang::Python => matches!(
            word,
            "def" | "class" | "if" | "elif" | "else" | "for" | "while" | "return"
                | "import" | "from" | "as" | "with" | "try" | "except" | "True" | "False"
                | "None" | "and" | "or" | "not" | "in" | "pass" | "break" | "continue"
                | "async" | "await" | "lambda"
        ),
        Lang::Cpp => matches!(
            word,
            "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "break"
                | "continue" | "return" | "struct" | "class" | "enum" | "const" | "static"
                | "void" | "int" | "char" | "bool" | "float" | "double" | "auto" | "fn"
                | "let" | "mut" | "pub" | "use" | "mod" | "impl" | "match" | "true"
                | "false" | "nullptr" | "NULL" | "include" | "define"
        ),
        Lang::JsTs => matches!(
            word,
            "function" | "const" | "let" | "var" | "if" | "else" | "for" | "while"
                | "return" | "import" | "export" | "from" | "class" | "interface"
                | "type" | "async" | "await" | "try" | "catch" | "new" | "this"
                | "true" | "false" | "null" | "undefined" | "switch" | "case"
        ),
        Lang::Json => matches!(word, "true" | "false" | "null"),
        Lang::Shell => matches!(
            word,
            "if" | "then" | "else" | "fi" | "for" | "do" | "done" | "echo" | "set"
                | "rem" | "REM" | "call" | "exit" | "export" | "function" | "cd"
        ),
        _ => false,
    }
}

pub fn highlight_line(lang: Lang, line: &str) -> Vec<(usize, usize, TokenKind)> {
    let mut out = Vec::new();
    if lang == Lang::Plain || line.is_empty() {
        return out;
    }
    let bytes = line.as_bytes();
    let len = bytes.len();

    if lang == Lang::Markdown {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            out.push((0, len, TokenKind::Keyword));
            return out;
        }
        if trimmed.starts_with('>') {
            out.push((0, len, TokenKind::Comment));
            return out;
        }
    }

    if lang == Lang::Ini {
        let ws = len - line.trim_start().len();
        if bytes.get(ws) == Some(&b';') || bytes.get(ws) == Some(&b'#') {
            out.push((ws, len, TokenKind::Comment));
            return out;
        }
        if bytes.get(ws) == Some(&b'[') {
            out.push((ws, len, TokenKind::Keyword));
            return out;
        }
    }

    let mut i = 0;
    while i < len {
        let b = bytes[i];
        if matches!(lang, Lang::Cpp | Lang::JsTs | Lang::Json) && i + 1 < len && b == b'/' && bytes[i + 1] == b'/' {
            out.push((i, len, TokenKind::Comment));
            break;
        }
        if matches!(lang, Lang::Python | Lang::Shell) && b == b'#' {
            out.push((i, len, TokenKind::Comment));
            break;
        }
        if lang == Lang::XmlHtml && i + 3 < len && &bytes[i..i + 4] == b"<!--" {
            out.push((i, len, TokenKind::Comment));
            break;
        }
        if b == b'"' || (b == b'\'' && lang != Lang::Markdown) {
            let quote = b;
            let start = i;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push((start, i, TokenKind::StringLit));
            continue;
        }
        if lang == Lang::XmlHtml && b == b'<' {
            let start = i;
            i += 1;
            while i < len && bytes[i] != b'>' && bytes[i] != b' ' {
                i += 1;
            }
            out.push((start, i, TokenKind::Keyword));
            continue;
        }
        if b.is_ascii_digit() && (i == 0 || !is_ident_char(bytes[i - 1])) {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_') {
                i += 1;
            }
            out.push((start, i, TokenKind::Number));
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < len && is_ident_char(bytes[i]) {
                i += 1;
            }
            let word = &line[start..i];
            if is_keyword(lang, word) {
                out.push((start, i, TokenKind::Keyword));
            } else if lang == Lang::Json && line[i..].trim_start().starts_with(':') {
                out.push((start, i, TokenKind::Accent));
            }
            continue;
        }
        i += 1;
    }
    out
}
