#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ============================================================================
// 1. 常量与菜单/控件 ID 定义
// ============================================================================

const LARGE_FILE_BYTES: usize = 20 * 1024 * 1024; // >20MB 触发大文件自动降级策略
const MAX_MATCH_HIGHLIGHTS: usize = 50_000;
const STATUS_BAR_HEIGHT: i32 = 24;
const FIND_PANEL_W: i32 = 430;
const FIND_PANEL_H: i32 = 108;
const CF_UNICODETEXT_ID: u32 = 13;

// 菜单命令 ID
const IDM_FILE_NEW: usize = 1001;
const IDM_FILE_OPEN: usize = 1002;
const IDM_FILE_SAVE: usize = 1003;
const IDM_FILE_SAVEAS: usize = 1004;
const IDM_FILE_RELOAD: usize = 1005;
const IDM_FILE_EXIT: usize = 1006;

const IDM_EDIT_UNDO: usize = 2001;
const IDM_EDIT_REDO: usize = 2002;
const IDM_EDIT_CUT: usize = 2003;
const IDM_EDIT_COPY: usize = 2004;
const IDM_EDIT_PASTE: usize = 2005;
const IDM_EDIT_SELECT_ALL: usize = 2006;
const IDM_EDIT_DUP_LINE: usize = 2007;
const IDM_EDIT_DEL_LINE: usize = 2008;
const IDM_EDIT_MOVE_UP: usize = 2009;
const IDM_EDIT_MOVE_DOWN: usize = 2010;
const IDM_EDIT_COMMENT: usize = 2011;
const IDM_EDIT_UPPER: usize = 2012;
const IDM_EDIT_LOWER: usize = 2013;

const IDM_SEARCH_FIND: usize = 3001;
const IDM_SEARCH_REPLACE: usize = 3002;
const IDM_SEARCH_GOTO: usize = 3003;
const IDM_SEARCH_NEXT: usize = 3004;
const IDM_SEARCH_PREV: usize = 3005;

const IDM_VIEW_WRAP: usize = 4001;
const IDM_VIEW_LINENUM: usize = 4002;
const IDM_VIEW_WHITESPACE: usize = 4003;
const IDM_VIEW_FONT: usize = 4004;
const IDM_VIEW_THEME: usize = 4005;
const IDM_VIEW_TOPMOST: usize = 4006;
const IDM_VIEW_ZOOM_IN: usize = 4007;
const IDM_VIEW_ZOOM_OUT: usize = 4008;
const IDM_VIEW_ZOOM_RESET: usize = 4009;

const IDM_ENC_REOPEN_UTF8: usize = 5001;
const IDM_ENC_REOPEN_UTF8BOM: usize = 5002;
const IDM_ENC_REOPEN_UTF16LE: usize = 5003;
const IDM_ENC_REOPEN_UTF16BE: usize = 5004;
const IDM_ENC_REOPEN_GBK: usize = 5005;
const IDM_ENC_REOPEN_BIG5: usize = 5006;

const IDM_ENC_CONV_UTF8: usize = 5101;
const IDM_ENC_CONV_UTF8BOM: usize = 5102;
const IDM_ENC_CONV_UTF16LE: usize = 5103;
const IDM_ENC_CONV_UTF16BE: usize = 5104;
const IDM_ENC_CONV_GBK: usize = 5105;
const IDM_ENC_CONV_BIG5: usize = 5106;

const IDM_EOL_CRLF: usize = 5201;
const IDM_EOL_LF: usize = 5202;

// 悬浮查找替换面板子控件 ID
const IDC_FIND_EDIT: i32 = 6001;
const IDC_REPLACE_EDIT: i32 = 6002;
const IDC_CHK_CASE: i32 = 6003;
const IDC_CHK_WORD: i32 = 6004;
const IDC_CHK_REGEX: i32 = 6005;
const IDC_BTN_PREV: i32 = 6006;
const IDC_BTN_NEXT: i32 = 6007;
const IDC_BTN_REPLACE: i32 = 6008;
const IDC_BTN_REPLACE_ALL: i32 = 6009;
const IDC_BTN_CLOSE: i32 = 6010;
const IDC_LBL_COUNT: i32 = 6011;

const IDC_GOTO_EDIT: i32 = 6101;
const IDC_GOTO_OK: i32 = 6102;
const IDC_GOTO_CANCEL: i32 = 6103;

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn from_wide_slice(slice: &[u16]) -> String {
    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..end])
}

// ============================================================================
// 2. 启动期错误日志与崩溃捕获（程序入口第一行即启用）
// ============================================================================

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn init_logger() {
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
        show_fatal_box("NotepadLite 启动/运行致命错误", &format!("{}\n\n详细信息已写入同级目录 app.log", msg));
    }));
}

fn log_msg(level: &str, msg: &str) {
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

fn show_fatal_box(title: &str, msg: &str) {
    let wt = to_wide(title);
    let wm = to_wide(msg);
    unsafe {
        MessageBoxW(null_mut(), wm.as_ptr(), wt.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

// ============================================================================
// 3. 绿色便携配置（严格保存在 exe 同级目录 config.json）
// ============================================================================

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct AppConfig {
    win_x: i32,
    win_y: i32,
    win_w: i32,
    win_h: i32,
    maximized: bool,
    font_name: String,
    font_size: i32,
    zoom_pct: i32,
    dark_theme: bool,
    word_wrap: bool,
    show_line_numbers: bool,
    show_whitespace: bool,
    always_on_top: bool,
    recent_files: Vec<String>,
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

fn load_config() -> AppConfig {
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

fn save_config(cfg: &AppConfig) {
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
// 4. 字符编码识别与转换（UTF-8 / UTF-8 BOM / UTF-16 LE&BE / GBK / Big5）
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum DocEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Gbk,
    Big5,
}

impl DocEncoding {
    fn label(self) -> &'static str {
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
enum LineEnding {
    Crlf,
    Lf,
}

impl LineEnding {
    fn label(self) -> &'static str {
        match self {
            LineEnding::Crlf => "CRLF",
            LineEnding::Lf => "LF",
        }
    }
}

fn normalize_newlines_fast(input: &str) -> (String, LineEnding) {
    let bytes = input.as_bytes();
    let has_cr = bytes.contains(&b'\r');
    if !has_cr {
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
    // 源字符串已是合法 UTF-8，仅剔除 ASCII '\r' 字节，结果仍保证合法 UTF-8
    let s = unsafe { String::from_utf8_unchecked(out) };
    (s, eol)
}

fn load_file_with_encoding(
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

fn save_file_with_encoding(
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
                // SAFETY: 逐字节拷贝时仅在 '\n' 前插入 '\r'，保留原 UTF-8 多字节序列完整
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
// 5. 大文件高性能文本缓冲、增量行索引、多光标与撤销重做引擎
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cursor {
    anchor: usize,
    head: usize,
}

impl Cursor {
    fn collapsed(pos: usize) -> Self {
        Self { anchor: pos, head: pos }
    }
    fn range(self) -> (usize, usize) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
    fn has_selection(self) -> bool {
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
    ops: Vec<SingleUndoOp>, // 按应用顺序排列（已按从后往前排序）
    cursors_before: Vec<Cursor>,
    cursors_after: Vec<Cursor>,
}

struct Document {
    text: String,
    line_starts: Vec<usize>,
    cursors: Vec<Cursor>,
    undo_stack: VecDeque<UndoEntry>,
    redo_stack: VecDeque<UndoEntry>,
    dirty: bool,
    path: Option<PathBuf>,
    encoding: DocEncoding,
    line_ending: LineEnding,
    disk_bytes: usize,
    large_mode: bool,
    total_chars: usize,
}

impl Document {
    fn new_empty() -> Self {
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

    fn rebuild_line_index(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);
        for (i, b) in self.text.bytes().enumerate() {
            if b == b'\n' {
                self.line_starts.push(i + 1);
            }
        }
        // 大文件避免全量 chars().count()，用字节数估算或仅中小文件精确统计
        self.total_chars = if self.text.len() > LARGE_FILE_BYTES {
            self.text.len()
        } else {
            self.text.chars().count()
        };
    }

    fn set_content(
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

    fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    fn line_of_byte(&self, pos: usize) -> usize {
        let p = pos.min(self.text.len());
        self.line_starts.partition_point(|&s| s <= p).saturating_sub(1)
    }

    fn line_bounds(&self, line: usize) -> (usize, usize) {
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

    fn line_str(&self, line: usize) -> &str {
        let (s, e) = self.line_bounds(line);
        &self.text[s..e]
    }

    fn byte_to_line_col(&self, pos: usize) -> (usize, usize) {
        let p = self.clamp_char_boundary(pos);
        let line = self.line_of_byte(p);
        let start = self.line_starts[line];
        let col = self.text[start..p].chars().count();
        (line, col)
    }

    fn line_col_to_byte(&self, line: usize, char_col: usize) -> usize {
        let l = line.min(self.line_count().saturating_sub(1));
        let (start, end) = self.line_bounds(l);
        let slice = &self.text[start..end];
        if let Some((idx, _)) = slice.char_indices().nth(char_col) {
            start + idx
        } else {
            end
        }
    }

    fn clamp_char_boundary(&self, pos: usize) -> usize {
        let mut p = pos.min(self.text.len());
        while p > 0 && !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    fn prev_char_boundary(&self, pos: usize) -> usize {
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

    fn next_char_boundary(&self, pos: usize) -> usize {
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

    fn primary(&self) -> Cursor {
        self.cursors[0]
    }

    fn normalize_cursors(&mut self) {
        if self.cursors.is_empty() {
            self.cursors.push(Cursor::collapsed(0));
            return;
        }
        for c in &mut self.cursors {
            c.anchor = self.clamp_char_boundary(c.anchor);
            c.head = self.clamp_char_boundary(c.head);
        }
        if self.cursors.len() > 1 {
            self.cursors.sort_by_key(|c| c.range());
            let mut merged: Vec<Cursor> = Vec::with_capacity(self.cursors.len());
            for c in self.cursors.drain(..) {
                if let Some(last) = merged.last_mut() {
                    let (ls, le) = last.range();
                    let (cs, ce) = c.range();
                    if cs <= le && (cs == ce && ls == le || cs < le || (cs == le && !c.has_selection() && !last.has_selection())) {
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

    /// 增量修改单区间并局部更新行索引（100MB 大文件输入依旧亚毫秒级响应）
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

    /// 执行一批无重叠编辑，自动记录撤销栈并同步移动所有光标
    fn apply_edits(&mut self, mut edits: Vec<(usize, usize, String)>, new_cursors: Option<Vec<Cursor>>) {
        if edits.is_empty() {
            return;
        }
        // 按起始偏移降序排列，从后往前修改不会破坏前方偏移
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
            // 默认将光标置于各处插入结束位置
            let mut next_cursors = Vec::new();
            // undo_ops 按降序，反向推算各编辑点最终偏移
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

    fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop_back() else {
            return false;
        };
        // entry.ops 是按 pos 降序记录的；撤销时从前往后（rev）还原，避免偏移错乱
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

    fn redo(&mut self) -> bool {
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

    fn insert_text(&mut self, input: &str) {
        let mut edits = Vec::with_capacity(self.cursors.len());
        for c in &self.cursors {
            let (s, e) = c.range();
            edits.push((s, e, input.to_string()));
        }
        self.apply_edits(edits, None);
    }

    fn backspace(&mut self) {
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

    fn delete_forward(&mut self) {
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

    fn selected_text_joined(&self) -> String {
        let mut parts = Vec::new();
        for c in &self.cursors {
            let (s, e) = c.range();
            if s < e {
                parts.push(&self.text[s..e]);
            }
        }
        parts.join("\r\n")
    }

    fn selected_char_count(&self) -> usize {
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

    fn duplicate_current_line(&mut self) {
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

    fn delete_current_line(&mut self) {
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

    fn move_lines(&mut self, up: bool) {
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

    fn toggle_comment(&mut self, lang: Lang) {
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

    fn transform_case(&mut self, upper: bool) {
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
// 6. 查找与正则替换引擎
// ============================================================================

struct SearchState {
    visible: bool,
    show_replace: bool,
    query: String,
    replace_with: String,
    case_sensitive: bool,
    whole_word: bool,
    use_regex: bool,
    matches: Vec<(usize, usize)>,
    current_idx: Option<usize>,
    last_error: Option<String>,
}

impl SearchState {
    fn new() -> Self {
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

    fn recompute(&mut self, text: &str, caret: usize) {
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
            let idx = self
                .matches
                .iter()
                .position(|&(s, _)| s >= caret)
                .unwrap_or(0);
            self.current_idx = Some(idx);
        }
    }

    fn step(&mut self, forward: bool) -> Option<(usize, usize)> {
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
// 7. 轻量级可见行即时语法高亮器（O(屏幕可见行数)，大文件零卡顿）
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
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
enum TokenKind {
    Keyword,
    StringLit,
    Comment,
    Number,
    Accent,
}

fn detect_lang(path: Option<&Path>) -> Lang {
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

/// 仅扫描单行（用于可见区即时着色），返回不重叠的高亮区间
fn highlight_line(lang: Lang, line: &str) -> Vec<(usize, usize, TokenKind)> {
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
        // 行注释
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
        // 字符串字面量
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
        // XML/HTML 标签
        if lang == Lang::XmlHtml && b == b'<' {
            let start = i;
            i += 1;
            while i < len && bytes[i] != b'>' && bytes[i] != b' ' {
                i += 1;
            }
            out.push((start, i, TokenKind::Keyword));
            continue;
        }
        // 数字
        if b.is_ascii_digit() && (i == 0 || !is_ident_char(bytes[i - 1])) {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_') {
                i += 1;
            }
            out.push((start, i, TokenKind::Number));
            continue;
        }
        // 标识符 / 关键词
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

// === [PART 2 接续位置：以下为 Win32 界面、双缓冲渲染、IME、快捷键与 main()] ===

// ============================================================================
// 8. 主题配色、Win32 辅助工具与原生编辑器界面状态
// ============================================================================

#[derive(Clone, Copy)]
struct Theme {
    bg: u32,
    gutter_bg: u32,
    gutter_fg: u32,
    cur_line_bg: u32,
    sel_bg: u32,
    match_bg: u32,
    cur_match_bg: u32,
    bracket_bg: u32,
    text_fg: u32,
    caret: u32,
    ws_fg: u32,
    status_bg: u32,
    status_fg: u32,
    status_border: u32,
    kw_fg: u32,
    str_fg: u32,
    cmt_fg: u32,
    num_fg: u32,
    acc_fg: u32,
}

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

fn get_theme(dark: bool) -> Theme {
    if dark {
        Theme {
            bg: rgb(30, 30, 30),
            gutter_bg: rgb(37, 37, 38),
            gutter_fg: rgb(133, 133, 133),
            cur_line_bg: rgb(42, 45, 48),
            sel_bg: rgb(38, 79, 120),
            match_bg: rgb(90, 70, 30),
            cur_match_bg: rgb(160, 110, 30),
            bracket_bg: rgb(65, 95, 65),
            text_fg: rgb(212, 212, 212),
            caret: rgb(235, 235, 235),
            ws_fg: rgb(85, 85, 85),
            status_bg: rgb(0, 122, 204),
            status_fg: rgb(255, 255, 255),
            status_border: rgb(45, 45, 48),
            kw_fg: rgb(86, 156, 214),
            str_fg: rgb(206, 145, 120),
            cmt_fg: rgb(106, 153, 85),
            num_fg: rgb(181, 206, 168),
            acc_fg: rgb(156, 220, 254),
        }
    } else {
        Theme {
            bg: rgb(255, 255, 255),
            gutter_bg: rgb(245, 245, 245),
            gutter_fg: rgb(120, 120, 120),
            cur_line_bg: rgb(240, 246, 255),
            sel_bg: rgb(173, 214, 255),
            match_bg: rgb(255, 235, 150),
            cur_match_bg: rgb(255, 180, 70),
            bracket_bg: rgb(205, 238, 205),
            text_fg: rgb(30, 30, 30),
            caret: rgb(0, 0, 0),
            ws_fg: rgb(195, 195, 195),
            status_bg: rgb(240, 240, 240),
            status_fg: rgb(45, 45, 45),
            status_border: rgb(215, 215, 215),
            kw_fg: rgb(0, 0, 220),
            str_fg: rgb(163, 21, 21),
            cmt_fg: rgb(0, 128, 0),
            num_fg: rgb(9, 134, 88),
            acc_fg: rgb(4, 81, 165),
        }
    }
}

#[inline]
fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}
#[inline]
fn hiword(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}
#[inline]
fn get_x_lparam(lp: isize) -> i32 {
    (lp & 0xFFFF) as i16 as i32
}
#[inline]
fn get_y_lparam(lp: isize) -> i32 {
    ((lp >> 16) & 0xFFFF) as i16 as i32
}
#[inline]
fn key_down(vk: u16) -> bool {
    unsafe { (GetKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

#[derive(Clone, Copy)]
struct VisualSeg {
    line: usize,
    b_start: usize, // 相对于行首的字节偏移
    b_end: usize,
}

struct AppState {
    hwnd: HWND,
    hfont_editor: HFONT,
    hfont_ui: HFONT,
    doc: Document,
    cfg: AppConfig,
    search: SearchState,
    lang: Lang,

    client_w: i32,
    client_h: i32,
    line_h: i32,
    avg_char_w: i32,
    gutter_w: i32,

    scroll_y: usize, // 顶部第一行(视觉行)
    scroll_x: i32,   // 像素水平滚动
    visual_lines: Vec<VisualSeg>,

    dragging: bool,
    rect_selecting: bool,
    rect_anchor: (usize, usize), // (line, visual_col)

    // 悬浮查找替换子面板控件句柄
    find_panel: HWND,
    find_edit: HWND,
    replace_edit: HWND,
    chk_case: HWND,
    chk_word: HWND,
    chk_regex: HWND,
    lbl_count: HWND,
    orig_find_edit_proc: isize,
    orig_rep_edit_proc: isize,

    // 转到行弹窗
    goto_hwnd: HWND,
    goto_edit: HWND,
}

impl AppState {
    fn effective_font_px(&self) -> i32 {
        ((self.cfg.font_size * self.cfg.zoom_pct) / 100).clamp(8, 72)
    }

    fn recreate_fonts(&mut self) {
        unsafe {
            if !self.hfont_editor.is_null() {
                DeleteObject(self.hfont_editor as _);
            }
            let px = self.effective_font_px();
            let wname = to_wide(&self.cfg.font_name);
            self.hfont_editor = CreateFontW(
                -px,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                (FIXED_PITCH | FF_MODERN) as u32,
                wname.as_ptr(),
            );

            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_editor as _);
            let mut tm: TEXTMETRICW = std::mem::zeroed();
            GetTextMetricsW(hdc, &mut tm);
            self.line_h = (tm.tmHeight + tm.tmExternalLeading + 3).max(14);
            self.avg_char_w = tm.tmAveCharWidth.max(6);
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
        }
        self.update_gutter_width();
        self.rebuild_visual_lines();
    }

    fn update_gutter_width(&mut self) {
        if !self.cfg.show_line_numbers {
            self.gutter_w = 8;
            return;
        }
        let digits = self.doc.line_count().max(1).to_string().len().max(3);
        self.gutter_w = (digits as i32 * self.avg_char_w) + 22;
    }

    fn editor_height(&self) -> i32 {
        (self.client_h - STATUS_BAR_HEIGHT).max(self.line_h)
    }

    fn visible_row_count(&self) -> usize {
        ((self.editor_height() / self.line_h.max(1)).max(1)) as usize
    }

    fn wrap_active(&self) -> bool {
        self.cfg.word_wrap && !self.doc.large_mode
    }

    fn rebuild_visual_lines(&mut self) {
        self.visual_lines.clear();
        if !self.wrap_active() {
            return;
        }
        let usable_w = (self.client_w - self.gutter_w - 14).max(self.avg_char_w * 8);
        let max_cols = (usable_w / self.avg_char_w.max(1)).max(8) as usize;
        for line in 0..self.doc.line_count() {
            let s = self.doc.line_str(line);
            if s.is_empty() {
                self.visual_lines.push(VisualSeg { line, b_start: 0, b_end: 0 });
                continue;
            }
            let mut seg_start = 0usize;
            let mut cols = 0usize;
            for (idx, ch) in s.char_indices() {
                let cw = if ch == '\t' {
                    4
                } else if (ch as u32) > 0x7F {
                    2
                } else {
                    1
                };
                if cols + cw > max_cols && idx > seg_start {
                    self.visual_lines.push(VisualSeg {
                        line,
                        b_start: seg_start,
                        b_end: idx,
                    });
                    seg_start = idx;
                    cols = 0;
                }
                cols += cw;
            }
            self.visual_lines.push(VisualSeg {
                line,
                b_start: seg_start,
                b_end: s.len(),
            });
        }
    }

    fn total_visual_lines(&self) -> usize {
        if self.wrap_active() {
            self.visual_lines.len().max(1)
        } else {
            self.doc.line_count().max(1)
        }
    }

    fn get_visual_seg(&self, vrow: usize) -> Option<VisualSeg> {
        if self.wrap_active() {
            self.visual_lines.get(vrow).copied()
        } else if vrow < self.doc.line_count() {
            let (s, e) = self.doc.line_bounds(vrow);
            Some(VisualSeg {
                line: vrow,
                b_start: 0,
                b_end: e - s,
            })
        } else {
            None
        }
    }

    fn byte_to_visual_row(&self, byte_pos: usize) -> usize {
        let line = self.doc.line_of_byte(byte_pos);
        if !self.wrap_active() {
            return line;
        }
        let line_start = self.doc.line_starts[line];
        let rel = byte_pos.saturating_sub(line_start);
        let first = self.visual_lines.partition_point(|seg| seg.line < line);
        let mut i = first;
        while i < self.visual_lines.len() && self.visual_lines[i].line == line {
            let seg = self.visual_lines[i];
            if rel <= seg.b_end {
                return i;
            }
            i += 1;
        }
        first.min(self.visual_lines.len().saturating_sub(1))
    }

    fn update_title(&self) {
        let mark = if self.doc.dirty { "* " } else { "" };
        let title = if let Some(p) = &self.doc.path {
            let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("未命名");
            format!("{}{} ({}) - NotepadLite", mark, fname, p.display())
        } else {
            format!("{}无标题 - NotepadLite", mark)
        };
        let wt = to_wide(&title);
        unsafe {
            SetWindowTextW(self.hwnd, wt.as_ptr());
        }
    }

    fn update_scrollbars(&mut self) {
        let total = self.total_visual_lines();
        let page = self.visible_row_count();
        let max_y = total.saturating_sub(1);
        self.scroll_y = self.scroll_y.min(max_y.saturating_sub(page.saturating_sub(1)));

        unsafe {
            let mut si: SCROLLINFO = std::mem::zeroed();
            si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
            si.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
            si.nMin = 0;
            si.nMax = max_y as i32;
            si.nPage = page as u32;
            si.nPos = self.scroll_y as i32;
            SetScrollInfo(self.hwnd, SB_VERT, &si, 1);

            if self.wrap_active() {
                self.scroll_x = 0;
                si.nMax = 0;
                si.nPage = 1;
                si.nPos = 0;
                SetScrollInfo(self.hwnd, SB_HORZ, &si, 1);
            } else {
                let edit_w = (self.client_w - self.gutter_w).max(50);
                si.nMax = (edit_w * 4).max(4000);
                si.nPage = edit_w as u32;
                si.nPos = self.scroll_x;
                SetScrollInfo(self.hwnd, SB_HORZ, &si, 1);
            }
        }
    }

    fn ensure_caret_visible(&mut self) {
        let head = self.doc.primary().head;
        let vrow = self.byte_to_visual_row(head);
        let page = self.visible_row_count();
        if vrow < self.scroll_y {
            self.scroll_y = vrow;
        } else if vrow >= self.scroll_y + page {
            self.scroll_y = vrow + 1 - page;
        }

        if !self.wrap_active() {
            let x = self.measure_byte_x(head);
            let edit_w = (self.client_w - self.gutter_w - 20).max(40);
            if x < self.scroll_x {
                self.scroll_x = (x - 40).max(0);
            } else if x > self.scroll_x + edit_w {
                self.scroll_x = x - edit_w + 40;
            }
        }
        self.update_scrollbars();
    }

    fn measure_byte_x(&self, byte_pos: usize) -> i32 {
        let line = self.doc.line_of_byte(byte_pos);
        let seg_start = if self.wrap_active() {
            let vrow = self.byte_to_visual_row(byte_pos);
            let rel_start = self.visual_lines.get(vrow).map(|s| s.b_start).unwrap_or(0);
            self.doc.line_starts[line] + rel_start
        } else {
            self.doc.line_starts[line]
        };
        let end = byte_pos.max(seg_start).min(self.doc.text.len());
        let slice = &self.doc.text[seg_start..end];
        if slice.len() > 8192 {
            return (slice.len() as i32) * self.avg_char_w;
        }
        let expanded = expand_tabs(slice);
        let w = to_wide(&expanded);
        let char_len = (w.len() - 1) as i32;
        unsafe {
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_editor as _);
            let mut sz: SIZE = std::mem::zeroed();
            GetTextExtentPoint32W(hdc, w.as_ptr(), char_len, &mut sz);
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
            sz.cx
        }
    }

    fn hit_test(&self, mouse_x: i32, mouse_y: i32) -> (usize, usize, usize) {
        let rel_row = (mouse_y.max(0) / self.line_h.max(1)) as usize;
        let vrow = (self.scroll_y + rel_row).min(self.total_visual_lines().saturating_sub(1));
        let Some(seg) = self.get_visual_seg(vrow) else {
            return (0, 0, 0);
        };
        let line_start = self.doc.line_starts[seg.line];
        let seg_str = &self.doc.text[line_start + seg.b_start..line_start + seg.b_end];
        let target_x = (mouse_x - self.gutter_w - 6 + self.scroll_x).max(0);

        if seg_str.is_empty() {
            return (line_start + seg.b_start, seg.line, 0);
        }
        if seg_str.len() > 8192 {
            let approx_chars = (target_x / self.avg_char_w.max(1)) as usize;
            let byte_off = seg_str
                .char_indices()
                .nth(approx_chars)
                .map(|(i, _)| i)
                .unwrap_or(seg_str.len());
            let abs = line_start + seg.b_start + byte_off;
            let (_, col) = self.doc.byte_to_line_col(abs);
            return (abs, seg.line, col);
        }

        // 收集所有字符边界用于精确二分命中测试（中英文混排无偏移）
        let mut bounds: Vec<usize> = seg_str.char_indices().map(|(i, _)| i).collect();
        bounds.push(seg_str.len());

        let mut best_idx = 0usize;
        unsafe {
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_editor as _);
            let mut lo = 0usize;
            let mut hi = bounds.len() - 1;
            while lo < hi {
                let mid = (lo + hi + 1) / 2;
                let w = to_wide(&expand_tabs(&seg_str[..bounds[mid]]));
                let mut sz: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(hdc, w.as_ptr(), (w.len() - 1) as i32, &mut sz);
                if sz.cx <= target_x {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            best_idx = lo;
            if lo + 1 < bounds.len() {
                let w1 = to_wide(&expand_tabs(&seg_str[..bounds[lo]]));
                let w2 = to_wide(&expand_tabs(&seg_str[..bounds[lo + 1]]));
                let mut s1: SIZE = std::mem::zeroed();
                let mut s2: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(hdc, w1.as_ptr(), (w1.len() - 1) as i32, &mut s1);
                GetTextExtentPoint32W(hdc, w2.as_ptr(), (w2.len() - 1) as i32, &mut s2);
                if (target_x - s1.cx) > (s2.cx - target_x) {
                    best_idx = lo + 1;
                }
            }
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
        }
        let abs = line_start + seg.b_start + bounds[best_idx];
        let (_, col) = self.doc.byte_to_line_col(abs);
        (abs, seg.line, col)
    }

    fn invalidate(&self) {
        unsafe {
            InvalidateRect(self.hwnd, null(), 0);
        }
    }

    fn update_ime_pos(&self) {
        let head = self.doc.primary().head;
        let vrow = self.byte_to_visual_row(head);
        let y = (vrow as isize - self.scroll_y as isize) as i32 * self.line_h;
        let x = self.gutter_w + 6 + self.measure_byte_x(head) - self.scroll_x;
        unsafe {
            let himc = ImmGetContext(self.hwnd);
            if !himc.is_null() {
                let cf = COMPOSITIONFORM {
                    dwStyle: CFS_POINT,
                    ptCurrentPos: POINT { x: x.max(0), y: y.max(0) },
                    rcArea: RECT { left: 0, top: 0, right: 0, bottom: 0 },
                };
                ImmSetCompositionWindow(himc, &cf);
                ImmReleaseContext(self.hwnd, himc);
            }
        }
    }

    fn refresh_search_ui(&mut self) {
        let caret = self.doc.primary().head;
        self.search.recompute(&self.doc.text, caret);
        let text = if let Some(err) = &self.search.last_error {
            format!("正则错误: {}", err)
        } else if self.search.query.is_empty() {
            "0 / 0".to_string()
        } else if self.search.matches.is_empty() {
            "无匹配".to_string()
        } else {
            let cur = self.search.current_idx.map(|i| i + 1).unwrap_or(0);
            format!("{} / {}", cur, self.search.matches.len())
        };
        let wt = to_wide(&text);
        unsafe {
            SetWindowTextW(self.lbl_count, wt.as_ptr());
        }
        self.invalidate();
    }

    fn show_find_panel(&mut self, with_replace: bool) {
        self.search.visible = true;
        self.search.show_replace = with_replace;
        // 若当前选中单行短文本，自动填入查找框
        let (s, e) = self.doc.primary().range();
        if s < e && (e - s) < 200 && !self.doc.text[s..e].contains('\n') {
            let sel = self.doc.text[s..e].to_string();
            self.search.query = sel.clone();
            let ws = to_wide(&sel);
            unsafe {
                SetWindowTextW(self.find_edit, ws.as_ptr());
            }
        }
        self.layout_find_panel();
        unsafe {
            ShowWindow(self.find_panel, SW_SHOW);
            SetFocus(self.find_edit);
            SendMessageW(self.find_edit, EM_SETSEL, 0, -1);
        }
        self.refresh_search_ui();
    }

    fn hide_find_panel(&mut self) {
        self.search.visible = false;
        unsafe {
            ShowWindow(self.find_panel, SW_HIDE);
            SetFocus(self.hwnd);
        }
        self.invalidate();
    }

    fn layout_find_panel(&self) {
        if self.find_panel.is_null() {
            return;
        }
        let h = if self.search.show_replace { FIND_PANEL_H } else { 70 };
        let x = (self.client_w - FIND_PANEL_W - 22).max(10);
        unsafe {
            MoveWindow(self.find_panel, x, 6, FIND_PANEL_W, h, 1);
            let rep_vis = if self.search.show_replace { SW_SHOW } else { SW_HIDE };
            ShowWindow(self.replace_edit, rep_vis);
            ShowWindow(GetDlgItem(self.find_panel, IDC_BTN_REPLACE), rep_vis);
            ShowWindow(GetDlgItem(self.find_panel, IDC_BTN_REPLACE_ALL), rep_vis);
        }
    }

    fn open_file_path(&mut self, path: &Path, forced_enc: Option<DocEncoding>) {
        match load_file_with_encoding(path, forced_enc) {
            Ok((text, enc, eol, size)) => {
                self.doc.set_content(text, Some(path.to_path_buf()), enc, eol, size);
                self.lang = detect_lang(Some(path));
                self.scroll_y = 0;
                self.scroll_x = 0;
                self.update_gutter_width();
                self.rebuild_visual_lines();
                self.update_scrollbars();
                self.update_title();

                let pstr = path.to_string_lossy().to_string();
                self.cfg.recent_files.retain(|r| r != &pstr);
                self.cfg.recent_files.insert(0, pstr);
                self.cfg.recent_files.truncate(10);
                save_config(&self.cfg);

                if self.search.visible {
                    self.refresh_search_ui();
                }
                self.invalidate();
            }
            Err(err) => {
                log_msg("ERROR", &format!("open_file_path({}): {}", path.display(), err));
                show_fatal_box("打开文件失败", &err);
            }
        }
    }

    fn confirm_discard(&mut self) -> bool {
        if !self.doc.dirty {
            return true;
        }
        let wt = to_wide("NotepadLite");
        let wm = to_wide("当前文档尚未保存，是否先保存修改？");
        let res = unsafe { MessageBoxW(self.hwnd, wm.as_ptr(), wt.as_ptr(), MB_YESNOCANCEL | MB_ICONQUESTION) };
        match res {
            IDYES => self.do_save(false),
            IDNO => true,
            _ => false,
        }
    }

    fn do_save(&mut self, save_as: bool) -> bool {
        let target = if !save_as {
            self.doc.path.clone()
        } else {
            None
        };
        let path = match target {
            Some(p) => p,
            None => match dialog_save_file(self.hwnd) {
                Some(p) => p,
                None => return false,
            },
        };
        match save_file_with_encoding(&path, &self.doc.text, self.doc.encoding, self.doc.line_ending) {
            Ok(bytes) => {
                self.doc.path = Some(path.clone());
                self.doc.dirty = false;
                self.doc.disk_bytes = bytes;
                self.lang = detect_lang(Some(&path));
                self.update_title();
                self.invalidate();
                true
            }
            Err(err) => {
                log_msg("ERROR", &format!("save_file({}): {}", path.display(), err));
                show_fatal_box("保存文件失败", &err);
                false
            }
        }
    }
}

fn expand_tabs(s: &str) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        if ch == '\t' {
            out.push_str("    ");
        } else {
            out.push(ch);
        }
    }
    out
}

// ============================================================================
// 9. 双缓冲 GDI 虚拟滚动渲染器（仅重绘可见行 + 状态栏）
// ============================================================================

fn find_matching_bracket(doc: &Document, pos: usize) -> Option<(usize, usize)> {
    let bytes = doc.text.as_bytes();
    let check_at = |idx: usize| -> Option<(usize, u8, u8, bool)> {
        if idx >= bytes.len() {
            return None;
        }
        match bytes[idx] {
            b'(' => Some((idx, b'(', b')', true)),
            b'[' => Some((idx, b'[', b']', true)),
            b'{' => Some((idx, b'{', b'}', true)),
            b')' => Some((idx, b')', b'(', false)),
            b']' => Some((idx, b']', b'[', false)),
            b'}' => Some((idx, b'}', b'{', false)),
            _ => None,
        }
    };
    let (start, open_b, close_b, forward) = check_at(pos).or_else(|| {
        if pos > 0 { check_at(pos - 1) } else { None }
    })?;

    let mut depth = 0i32;
    let limit = 15_000usize;
    if forward {
        let end = (start + limit).min(bytes.len());
        for i in start..end {
            if bytes[i] == open_b {
                depth += 1;
            } else if bytes[i] == close_b {
                depth -= 1;
                if depth == 0 {
                    return Some((start, i));
                }
            }
        }
    } else {
        let begin = start.saturating_sub(limit);
        for i in (begin..=start).rev() {
            if bytes[i] == open_b {
                depth += 1;
            } else if bytes[i] == close_b {
                depth -= 1;
                if depth == 0 {
                    return Some((i, start));
                }
            }
        }
    }
    None
}

fn paint_editor(state: &AppState, hdc_win: HDC) {
    let w = state.client_w.max(1);
    let h = state.client_h.max(1);
    let theme = get_theme(state.cfg.dark_theme);

    unsafe {
        let mem_dc = CreateCompatibleDC(hdc_win);
        let bmp = CreateCompatibleBitmap(hdc_win, w, h);
        let old_bmp = SelectObject(mem_dc, bmp as _);
        let old_font = SelectObject(mem_dc, state.hfont_editor as _);
        SetBkMode(mem_dc, TRANSPARENT as i32);

        // 背景填充
        let bg_br = CreateSolidBrush(theme.bg);
        let full_rc = RECT { left: 0, top: 0, right: w, bottom: h - STATUS_BAR_HEIGHT };
        FillRect(mem_dc, &full_rc, bg_br);
        DeleteObject(bg_br as _);

        // 行号槽背景
        let gut_br = CreateSolidBrush(theme.gutter_bg);
        let gut_rc = RECT { left: 0, top: 0, right: state.gutter_w, bottom: h - STATUS_BAR_HEIGHT };
        FillRect(mem_dc, &gut_rc, gut_br);
        DeleteObject(gut_br as _);

        let cur_line = state.doc.line_of_byte(state.doc.primary().head);
        let bracket_pair = if !state.doc.large_mode {
            find_matching_bracket(&state.doc, state.doc.primary().head)
        } else {
            None
        };

        let edit_clip = RECT {
            left: state.gutter_w,
            top: 0,
            right: w,
            bottom: h - STATUS_BAR_HEIGHT,
        };
        let clip_rgn = CreateRectRgn(edit_clip.left, edit_clip.top, edit_clip.right, edit_clip.bottom);

        let vis_rows = state.visible_row_count() + 1;
        let cur_line_br = CreateSolidBrush(theme.cur_line_bg);
        let sel_br = CreateSolidBrush(theme.sel_bg);
        let match_br = CreateSolidBrush(theme.match_bg);
        let cur_match_br = CreateSolidBrush(theme.cur_match_bg);
        let brk_br = CreateSolidBrush(theme.bracket_bg);
        let caret_br = CreateSolidBrush(theme.caret);

        let mut last_drawn_logical = usize::MAX;
        let text_left = state.gutter_w + 6 - state.scroll_x;
        let active_lang = if state.doc.large_mode { Lang::Plain } else { state.lang };

        for r in 0..vis_rows {
            let vrow = state.scroll_y + r;
            let Some(seg) = state.get_visual_seg(vrow) else {
                break;
            };
            let y = (r as i32) * state.line_h;
            if y >= h - STATUS_BAR_HEIGHT {
                break;
            }

            // 当前行高亮背景
            if seg.line == cur_line {
                let row_rc = RECT {
                    left: state.gutter_w,
                    top: y,
                    right: w,
                    bottom: y + state.line_h,
                };
                FillRect(mem_dc, &row_rc, cur_line_br);
            }

            // 绘制行号（自动换行跨多行时仅在首行显示行号）
            if state.cfg.show_line_numbers && seg.line != last_drawn_logical {
                SetTextColor(mem_dc, if seg.line == cur_line { theme.text_fg } else { theme.gutter_fg });
                let num_s = to_wide(&(seg.line + 1).to_string());
                let mut nrc = RECT {
                    left: 2,
                    top: y + 1,
                    right: state.gutter_w - 8,
                    bottom: y + state.line_h,
                };
                DrawTextW(mem_dc, num_s.as_ptr(), (num_s.len() - 1) as i32, &mut nrc, DT_RIGHT | DT_SINGLELINE);
            }
            last_drawn_logical = seg.line;

            // 进入主编辑区裁剪
            SelectClipRgn(mem_dc, clip_rgn);

            let line_abs_start = state.doc.line_starts[seg.line];
            let seg_abs_start = line_abs_start + seg.b_start;
            let seg_abs_end = line_abs_start + seg.b_end;
            let full_line = state.doc.line_str(seg.line);
            let seg_str = &full_line[seg.b_start..seg.b_end];

            // 超长单行裁剪视口切片，防止 GDI 卡死
            let draw_slice = if seg_str.len() > 16_384 {
                let mut e = 16_384;
                while !seg_str.is_char_boundary(e) {
                    e -= 1;
                }
                &seg_str[..e]
            } else {
                seg_str
            };

            // 辅助函数：把段内字节偏移转为屏幕 X
            let seg_x = |abs_byte: usize| -> i32 {
                let rel = abs_byte.saturating_sub(seg_abs_start).min(draw_slice.len());
                let sub = &draw_slice[..rel];
                if sub.is_empty() {
                    return text_left;
                }
                let ew = to_wide(&expand_tabs(sub));
                let mut sz: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(mem_dc, ew.as_ptr(), (ew.len() - 1) as i32, &mut sz);
                text_left + sz.cx
            };

            // 1. 搜索命中高亮
            if state.search.visible && !state.search.matches.is_empty() {
                let lo = state.search.matches.partition_point(|m| m.1 <= seg_abs_start);
                for idx in lo..state.search.matches.len() {
                    let (ms, me) = state.search.matches[idx];
                    if ms >= seg_abs_end {
                        break;
                    }
                    let xs = seg_x(ms.max(seg_abs_start));
                    let xe = seg_x(me.min(seg_abs_end));
                    let mrc = RECT { left: xs, top: y, right: xe.max(xs + 2), bottom: y + state.line_h };
                    let br = if state.search.current_idx == Some(idx) { cur_match_br } else { match_br };
                    FillRect(mem_dc, &mrc, br);
                }
            }

            // 2. 括号匹配高亮
            if let Some((b1, b2)) = bracket_pair {
                for bp in [b1, b2] {
                    if bp >= seg_abs_start && bp < seg_abs_end {
                        let xs = seg_x(bp);
                        let xe = seg_x(bp + 1);
                        let brc = RECT { left: xs, top: y, right: xe, bottom: y + state.line_h };
                        FillRect(mem_dc, &brc, brk_br);
                    }
                }
            }

            // 3. 选区背景（支持多光标/列块选区）
            for c in &state.doc.cursors {
                let (cs, ce) = c.range();
                if cs < ce && ce > seg_abs_start && cs <= seg_abs_end {
                    let s_clamped = cs.max(seg_abs_start);
                    let e_clamped = ce.min(seg_abs_end);
                    let xs = seg_x(s_clamped);
                    let mut xe = seg_x(e_clamped);
                    if ce > seg_abs_end {
                        xe += state.avg_char_w / 2;
                    }
                    let src = RECT { left: xs, top: y, right: xe.max(xs + 2), bottom: y + state.line_h };
                    FillRect(mem_dc, &src, sel_br);
                }
            }

            // 4. 语法高亮与文本绘制
            let tokens = if draw_slice.len() <= 8192 {
                highlight_line(active_lang, full_line)
            } else {
                Vec::new()
            };

            let mut cur_x = text_left;
            let mut pos = 0usize; // 相对于 draw_slice
            let mut draw_chunk = |chunk: &str, color: u32, x_acc: &mut i32| {
                if chunk.is_empty() {
                    return;
                }
                let shown = if state.cfg.show_whitespace {
                    chunk.replace(' ', "·").replace('\t', "→   ")
                } else {
                    expand_tabs(chunk)
                };
                let wtxt = to_wide(&shown);
                let clen = (wtxt.len() - 1) as i32;
                if state.cfg.show_whitespace && chunk.chars().all(|ch| ch == ' ' || ch == '\t') {
                    SetTextColor(mem_dc, theme.ws_fg);
                } else {
                    SetTextColor(mem_dc, color);
                }
                TextOutW(mem_dc, *x_acc, y + 1, wtxt.as_ptr(), clen);
                let mut sz: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(mem_dc, wtxt.as_ptr(), clen, &mut sz);
                *x_acc += sz.cx;
            };

            for (ts, te, kind) in tokens {
                // 把全行 token 区间投影到当前段 [seg.b_start..seg.b_start+draw_slice.len()]
                let seg_end_rel = seg.b_start + draw_slice.len();
                if te <= seg.b_start || ts >= seg_end_rel {
                    continue;
                }
                let ls = ts.saturating_sub(seg.b_start);
                let le = (te.min(seg_end_rel)) - seg.b_start;
                if ls > pos {
                    draw_chunk(&draw_slice[pos..ls], theme.text_fg, &mut cur_x);
                }
                let tcol = match kind {
                    TokenKind::Keyword => theme.kw_fg,
                    TokenKind::StringLit => theme.str_fg,
                    TokenKind::Comment => theme.cmt_fg,
                    TokenKind::Number => theme.num_fg,
                    TokenKind::Accent => theme.acc_fg,
                };
                draw_chunk(&draw_slice[ls..le], tcol, &mut cur_x);
                pos = le;
            }
            if pos < draw_slice.len() {
                draw_chunk(&draw_slice[pos..], theme.text_fg, &mut cur_x);
            }

            // 5. 绘制全部光标竖线
            for c in &state.doc.cursors {
                let in_seg = if seg.b_end == full_line.len() {
                    c.head >= seg_abs_start && c.head <= seg_abs_end
                } else {
                    c.head >= seg_abs_start && c.head < seg_abs_end
                };
                if in_seg {
                    let cx = seg_x(c.head);
                    let crc = RECT { left: cx, top: y + 1, right: cx + 2, bottom: y + state.line_h - 1 };
                    FillRect(mem_dc, &crc, caret_br);
                }
            }

            SelectClipRgn(mem_dc, null_mut());
        }

        DeleteObject(clip_rgn as _);
        DeleteObject(cur_line_br as _);
        DeleteObject(sel_br as _);
        DeleteObject(match_br as _);
        DeleteObject(cur_match_br as _);
        DeleteObject(brk_br as _);
        DeleteObject(caret_br as _);

        // 6. 底部紧凑状态栏（单行双栏信息）
        let sb_top = h - STATUS_BAR_HEIGHT;
        let sb_br = CreateSolidBrush(theme.status_bg);
        let sb_rc = RECT { left: 0, top: sb_top, right: w, bottom: h };
        FillRect(mem_dc, &sb_rc, sb_br);
        DeleteObject(sb_br as _);

        let bd_br = CreateSolidBrush(theme.status_border);
        let bd_rc = RECT { left: 0, top: sb_top, right: w, bottom: sb_top + 1 };
        FillRect(mem_dc, &bd_rc, bd_br);
        DeleteObject(bd_br as _);

        SelectObject(mem_dc, state.hfont_ui as _);
        SetTextColor(mem_dc, theme.status_fg);

        let (cline, ccol) = state.doc.byte_to_line_col(state.doc.primary().head);
        let sel_cnt = state.doc.selected_char_count();
        let multi_tag = if state.doc.cursors.len() > 1 {
            format!("  |  多光标: {}", state.doc.cursors.len())
        } else {
            String::new()
        };
        let large_tag = if state.doc.large_mode { "  [大文件极速]" } else { "" };
        let left_info = format!(
            "  行 {} / {}   列 {}   选中 {}{}{}",
            cline + 1,
            state.doc.line_count(),
            ccol + 1,
            sel_cnt,
            multi_tag,
            large_tag
        );
        let right_info = format!(
            "字数 {} ({} 字节)   |   {}   |   {}   |   {}%  ",
            state.doc.total_chars,
            state.doc.text.len(),
            state.doc.encoding.label(),
            state.doc.line_ending.label(),
            state.cfg.zoom_pct
        );

        let wl = to_wide(&left_info);
        let mut lrc = RECT { left: 4, top: sb_top + 3, right: w / 2 + 80, bottom: h };
        DrawTextW(mem_dc, wl.as_ptr(), (wl.len() - 1) as i32, &mut lrc, DT_LEFT | DT_SINGLELINE);

        let wr = to_wide(&right_info);
        let mut rrc = RECT { left: w / 3, top: sb_top + 3, right: w - 6, bottom: h };
        DrawTextW(mem_dc, wr.as_ptr(), (wr.len() - 1) as i32, &mut rrc, DT_RIGHT | DT_SINGLELINE);

        SelectObject(mem_dc, old_font);
        BitBlt(hdc_win, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
        SelectObject(mem_dc, old_bmp);
        DeleteObject(bmp as _);
        DeleteDC(mem_dc);
    }
}

// ============================================================================
// 10. 剪贴板、系统原生打开/保存/字体对话框
// ============================================================================

fn clipboard_copy(hwnd: HWND, text: &str) {
    if text.is_empty() {
        return;
    }
    let wide = to_wide(text);
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return;
        }
        EmptyClipboard();
        let bytes = wide.len() * 2;
        let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !hmem.is_null() {
            let ptr = GlobalLock(hmem) as *mut u16;
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                GlobalUnlock(hmem);
                SetClipboardData(CF_UNICODETEXT_ID, hmem);
            }
        }
        CloseClipboard();
    }
}

fn clipboard_paste(hwnd: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return None;
        }
        let hdata = GetClipboardData(CF_UNICODETEXT_ID);
        let mut res = None;
        if !hdata.is_null() {
            let ptr = GlobalLock(hdata) as *const u16;
            if !ptr.is_null() {
                let mut len = 0usize;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(ptr, len);
                let (norm, _) = normalize_newlines_fast(&String::from_utf16_lossy(slice));
                res = Some(norm);
                GlobalUnlock(hdata);
            }
        }
        CloseClipboard();
        res
    }
}

fn dialog_open_file(hwnd: HWND) -> Option<PathBuf> {
    let mut buf = vec![0u16; 4096];
    let filter: Vec<u16> = "所有文件 (*.*)\0*.*\0文本与代码 (*.txt;*.log;*.md;*.json;*.py;*.rs;*.c;*.cpp;*.js;*.ts)\0*.txt;*.log;*.md;*.json;*.py;*.rs;*.c;*.cpp;*.js;*.ts\0\0"
        .encode_utf16()
        .collect();
    unsafe {
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = hwnd;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = buf.as_mut_ptr();
        ofn.nMaxFile = buf.len() as u32;
        ofn.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_HIDEREADONLY;
        if GetOpenFileNameW(&mut ofn) != 0 {
            return Some(PathBuf::from(from_wide_slice(&buf)));
        }
    }
    None
}

fn dialog_save_file(hwnd: HWND) -> Option<PathBuf> {
    let mut buf = vec![0u16; 4096];
    let filter: Vec<u16> = "所有文件 (*.*)\0*.*\0文本文档 (*.txt)\0*.txt\0\0"
        .encode_utf16()
        .collect();
    unsafe {
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = hwnd;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = buf.as_mut_ptr();
        ofn.nMaxFile = buf.len() as u32;
        ofn.Flags = OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_HIDEREADONLY;
        if GetSaveFileNameW(&mut ofn) != 0 {
            return Some(PathBuf::from(from_wide_slice(&buf)));
        }
    }
    None
}

fn dialog_choose_font(state: &mut AppState) {
    unsafe {
        let mut lf: LOGFONTW = std::mem::zeroed();
        lf.lfHeight = -state.cfg.font_size;
        lf.lfWeight = FW_NORMAL as i32;
        let wname = to_wide(&state.cfg.font_name);
        let n = wname.len().min(32);
        lf.lfFaceName[..n].copy_from_slice(&wname[..n]);

        let mut cf: CHOOSEFONTW = std::mem::zeroed();
        cf.lStructSize = std::mem::size_of::<CHOOSEFONTW>() as u32;
        cf.hwndOwner = state.hwnd;
        cf.lpLogFont = &mut lf;
        cf.Flags = CF_SCREENFONTS | CF_INITTOLOGFONTSTRUCT;
        if ChooseFontW(&mut cf) != 0 {
            state.cfg.font_name = from_wide_slice(&lf.lfFaceName);
            state.cfg.font_size = (cf.iPointSize / 10).max(8);
            state.cfg.zoom_pct = 100;
            state.recreate_fonts();
            save_config(&state.cfg);
            state.invalidate();
        }
    }
}

// ============================================================================
// 11. 菜单构建、悬浮查找面板、转到行弹窗与快捷键分发
// ============================================================================

fn build_main_menu() -> HMENU {
    unsafe {
        let root = CreateMenu();
        let add = |menu: HMENU, id: usize, label: &str| {
            let w = to_wide(label);
            AppendMenuW(menu, MF_STRING, id, w.as_ptr());
        };
        let add_pop = |parent: HMENU, child: HMENU, label: &str| {
            let w = to_wide(label);
            AppendMenuW(parent, MF_POPUP, child as usize, w.as_ptr());
        };

        let m_file = CreatePopupMenu();
        add(m_file, IDM_FILE_NEW, "新建(&N)\tCtrl+N");
        add(m_file, IDM_FILE_OPEN, "打开(&O)...\tCtrl+O");
        add(m_file, IDM_FILE_SAVE, "保存(&S)\tCtrl+S");
        add(m_file, IDM_FILE_SAVEAS, "另存为(&A)...\tCtrl+Shift+S");
        add(m_file, IDM_FILE_RELOAD, "从磁盘重新加载(&R)\tF5");
        AppendMenuW(m_file, MF_SEPARATOR, 0, null());
        add(m_file, IDM_FILE_EXIT, "退出(&X)\tAlt+F4");
        add_pop(root, m_file, "文件(&F)");

        let m_edit = CreatePopupMenu();
        add(m_edit, IDM_EDIT_UNDO, "撤销(&U)\tCtrl+Z");
        add(m_edit, IDM_EDIT_REDO, "重做(&R)\tCtrl+Y");
        AppendMenuW(m_edit, MF_SEPARATOR, 0, null());
        add(m_edit, IDM_EDIT_CUT, "剪切(&T)\tCtrl+X");
        add(m_edit, IDM_EDIT_COPY, "复制(&C)\tCtrl+C");
        add(m_edit, IDM_EDIT_PASTE, "粘贴(&P)\tCtrl+V");
        add(m_edit, IDM_EDIT_SELECT_ALL, "全选(&A)\tCtrl+A");
        AppendMenuW(m_edit, MF_SEPARATOR, 0, null());
        add(m_edit, IDM_EDIT_DUP_LINE, "复制当前行\tCtrl+D");
        add(m_edit, IDM_EDIT_DEL_LINE, "删除当前行\tCtrl+Shift+K");
        add(m_edit, IDM_EDIT_MOVE_UP, "上移当前行\tAlt+Up");
        add(m_edit, IDM_EDIT_MOVE_DOWN, "下移当前行\tAlt+Down");
        add(m_edit, IDM_EDIT_COMMENT, "切换单行注释\tCtrl+/");
        AppendMenuW(m_edit, MF_SEPARATOR, 0, null());
        add(m_edit, IDM_EDIT_UPPER, "转为大写\tCtrl+Shift+U");
        add(m_edit, IDM_EDIT_LOWER, "转为小写\tCtrl+U");
        add_pop(root, m_edit, "编辑(&E)");

        let m_search = CreatePopupMenu();
        add(m_search, IDM_SEARCH_FIND, "查找(&F)...\tCtrl+F");
        add(m_search, IDM_SEARCH_REPLACE, "替换(&R)...\tCtrl+H");
        add(m_search, IDM_SEARCH_NEXT, "查找下一个(&N)\tF3");
        add(m_search, IDM_SEARCH_PREV, "查找上一个(&P)\tShift+F3");
        AppendMenuW(m_search, MF_SEPARATOR, 0, null());
        add(m_search, IDM_SEARCH_GOTO, "转到指定行(&G)...\tCtrl+G");
        add_pop(root, m_search, "搜索(&S)");

        let m_view = CreatePopupMenu();
        add(m_view, IDM_VIEW_WRAP, "自动换行(&W)\tAlt+Z");
        add(m_view, IDM_VIEW_LINENUM, "显示行号(&L)");
        add(m_view, IDM_VIEW_WHITESPACE, "显示空白字符(&S)");
        AppendMenuW(m_view, MF_SEPARATOR, 0, null());
        add(m_view, IDM_VIEW_FONT, "字体与字号设置(&F)...");
        add(m_view, IDM_VIEW_THEME, "切换深色 / 浅色主题(&T)");
        add(m_view, IDM_VIEW_TOPMOST, "窗口置顶(&A)");
        AppendMenuW(m_view, MF_SEPARATOR, 0, null());
        add(m_view, IDM_VIEW_ZOOM_IN, "放大字体\tCtrl++");
        add(m_view, IDM_VIEW_ZOOM_OUT, "缩小字体\tCtrl+-");
        add(m_view, IDM_VIEW_ZOOM_RESET, "重置缩放 (100%)\tCtrl+0");
        add_pop(root, m_view, "视图(&V)");

        let m_enc = CreatePopupMenu();
        let m_reopen = CreatePopupMenu();
        add(m_reopen, IDM_ENC_REOPEN_UTF8, "UTF-8");
        add(m_reopen, IDM_ENC_REOPEN_UTF8BOM, "UTF-8 (带 BOM)");
        add(m_reopen, IDM_ENC_REOPEN_UTF16LE, "UTF-16 LE");
        add(m_reopen, IDM_ENC_REOPEN_UTF16BE, "UTF-16 BE");
        add(m_reopen, IDM_ENC_REOPEN_GBK, "GBK / GB18030");
        add(m_reopen, IDM_ENC_REOPEN_BIG5, "Big5");
        add_pop(m_enc, m_reopen, "以指定编码重新打开(&R)");

        let m_conv = CreatePopupMenu();
        add(m_conv, IDM_ENC_CONV_UTF8, "UTF-8");
        add(m_conv, IDM_ENC_CONV_UTF8BOM, "UTF-8 (带 BOM)");
        add(m_conv, IDM_ENC_CONV_UTF16LE, "UTF-16 LE");
        add(m_conv, IDM_ENC_CONV_UTF16BE, "UTF-16 BE");
        add(m_conv, IDM_ENC_CONV_GBK, "GBK / GB18030");
        add(m_conv, IDM_ENC_CONV_BIG5, "Big5");
        add_pop(m_enc, m_conv, "转换为指定编码保存(&C)");

        AppendMenuW(m_enc, MF_SEPARATOR, 0, null());
        add(m_enc, IDM_EOL_CRLF, "Windows (CRLF)");
        add(m_enc, IDM_EOL_LF, "Unix / Linux (LF)");
        add_pop(root, m_enc, "编码与换行(&C)");

        root
    }
}

fn sync_menu_checks(state: &AppState, hmenu: HMENU) {
    let chk = |id: usize, on: bool| unsafe {
        CheckMenuItem(hmenu, id as u32, MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED });
    };
    chk(IDM_VIEW_WRAP, state.cfg.word_wrap);
    chk(IDM_VIEW_LINENUM, state.cfg.show_line_numbers);
    chk(IDM_VIEW_WHITESPACE, state.cfg.show_whitespace);
    chk(IDM_VIEW_THEME, state.cfg.dark_theme);
    chk(IDM_VIEW_TOPMOST, state.cfg.always_on_top);
    chk(IDM_EOL_CRLF, state.doc.line_ending == LineEnding::Crlf);
    chk(IDM_EOL_LF, state.doc.line_ending == LineEnding::Lf);

    chk(IDM_ENC_CONV_UTF8, state.doc.encoding == DocEncoding::Utf8);
    chk(IDM_ENC_CONV_UTF8BOM, state.doc.encoding == DocEncoding::Utf8Bom);
    chk(IDM_ENC_CONV_UTF16LE, state.doc.encoding == DocEncoding::Utf16Le);
    chk(IDM_ENC_CONV_UTF16BE, state.doc.encoding == DocEncoding::Utf16Be);
    chk(IDM_ENC_CONV_GBK, state.doc.encoding == DocEncoding::Gbk);
    chk(IDM_ENC_CONV_BIG5, state.doc.encoding == DocEncoding::Big5);
}

fn get_win_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        from_wide_slice(&buf)
    }
}

unsafe extern "system" fn edit_box_subclass(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let panel = GetParent(hwnd);
    let main_hwnd = GetParent(panel);
    let state_ptr = GetWindowLongPtrW(main_hwnd, GWLP_USERDATA) as *mut AppState;
    let orig = if !state_ptr.is_null() {
        let st = &*state_ptr;
        if hwnd == st.find_edit { st.orig_find_edit_proc } else { st.orig_rep_edit_proc }
    } else {
        0
    };

    if msg == WM_KEYDOWN && !state_ptr.is_null() {
        let st = &mut *state_ptr;
        if wp == VK_ESCAPE as usize {
            st.hide_find_panel();
            return 0;
        }
        if wp == VK_RETURN as usize {
            let forward = !key_down(VK_SHIFT);
            if let Some((ms, me)) = st.search.step(forward) {
                st.doc.cursors = vec![Cursor { anchor: ms, head: me }];
                st.ensure_caret_visible();
                st.refresh_search_ui();
            }
            return 0;
        }
        if wp == VK_TAB as usize && st.search.show_replace {
            let target = if hwnd == st.find_edit { st.replace_edit } else { st.find_edit };
            SetFocus(target);
            SendMessageW(target, EM_SETSEL, 0, -1);
            return 0;
        }
    }
    CallWindowProcW(std::mem::transmute(orig), hwnd, msg, wp, lp)
}

unsafe extern "system" fn find_panel_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let main_hwnd = GetParent(hwnd);
    let state_ptr = GetWindowLongPtrW(main_hwnd, GWLP_USERDATA) as *mut AppState;
    if msg == WM_COMMAND && !state_ptr.is_null() {
        let st = &mut *state_ptr;
        let id = loword(wp) as i32;
        let code = hiword(wp) as u32;
        match id {
            IDC_FIND_EDIT if code == EN_CHANGE => {
                st.search.query = get_win_text(st.find_edit);
                st.refresh_search_ui();
                return 0;
            }
            IDC_REPLACE_EDIT if code == EN_CHANGE => {
                st.search.replace_with = get_win_text(st.replace_edit);
                return 0;
            }
            IDC_CHK_CASE | IDC_CHK_WORD | IDC_CHK_REGEX => {
                st.search.case_sensitive = SendMessageW(st.chk_case, BM_GETCHECK as u32, 0, 0) == BST_CHECKED as isize;
                st.search.whole_word = SendMessageW(st.chk_word, BM_GETCHECK as u32, 0, 0) == BST_CHECKED as isize;
                st.search.use_regex = SendMessageW(st.chk_regex, BM_GETCHECK as u32, 0, 0) == BST_CHECKED as isize;
                st.refresh_search_ui();
                return 0;
            }
            IDC_BTN_NEXT | IDC_BTN_PREV => {
                if let Some((ms, me)) = st.search.step(id == IDC_BTN_NEXT) {
                    st.doc.cursors = vec![Cursor { anchor: ms, head: me }];
                    st.ensure_caret_visible();
                    st.refresh_search_ui();
                }
                return 0;
            }
            IDC_BTN_REPLACE => {
                if let Some(idx) = st.search.current_idx {
                    if let Some(&(ms, me)) = st.search.matches.get(idx) {
                        let rep = st.search.replace_with.clone();
                        let rlen = rep.len();
                        st.doc.apply_edits(vec![(ms, me, rep)], Some(vec![Cursor::collapsed(ms + rlen)]));
                        st.update_gutter_width();
                        st.rebuild_visual_lines();
                        st.update_title();
                        st.refresh_search_ui();
                        if let Some((ns, ne)) = st.search.step(true) {
                            st.doc.cursors = vec![Cursor { anchor: ns, head: ne }];
                        }
                        st.ensure_caret_visible();
                    }
                }
                return 0;
            }
            IDC_BTN_REPLACE_ALL => {
                if !st.search.matches.is_empty() {
                    let rep = st.search.replace_with.clone();
                    let edits: Vec<(usize, usize, String)> = st
                        .search
                        .matches
                        .iter()
                        .map(|&(s, e)| (s, e, rep.clone()))
                        .collect();
                    st.doc.apply_edits(edits, None);
                    st.update_gutter_width();
                    st.rebuild_visual_lines();
                    st.ensure_caret_visible();
                    st.update_title();
                    st.refresh_search_ui();
                }
                return 0;
            }
            IDC_BTN_CLOSE => {
                st.hide_find_panel();
                return 0;
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn create_find_panel(state: &mut AppState, hinst: HINSTANCE) {
    unsafe {
        let cls = to_wide("NotepadLiteFindPanel");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(find_panel_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszMenuName: null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc);

        let panel = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            cls.as_ptr(),
            null(),
            WS_CHILD | WS_BORDER | WS_CLIPSIBLINGS,
            0,
            0,
            FIND_PANEL_W,
            FIND_PANEL_H,
            state.hwnd,
            null_mut(),
            hinst,
            null(),
        );
        state.find_panel = panel;

        let mk = |cls_name: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: i32| -> HWND {
            let wc = to_wide(cls_name);
            let wt = to_wide(text);
            let ch = CreateWindowExW(0, wc.as_ptr(), wt.as_ptr(), WS_CHILD | WS_VISIBLE | style, x, y, w, h, panel, id as isize as HMENU, hinst, null());
            SendMessageW(ch, WM_SETFONT, state.hfont_ui as usize, 1);
            ch
        };

        state.find_edit = mk("EDIT", "", WS_BORDER | ES_AUTOHSCROLL as u32, 8, 8, 200, 24, IDC_FIND_EDIT);
        state.lbl_count = mk("STATIC", "0 / 0", SS_CENTER as u32, 214, 12, 74, 20, IDC_LBL_COUNT);
        mk("BUTTON", "↑", BS_PUSHBUTTON as u32, 294, 8, 30, 24, IDC_BTN_PREV);
        mk("BUTTON", "↓", BS_PUSHBUTTON as u32, 328, 8, 30, 24, IDC_BTN_NEXT);
        mk("BUTTON", "×", BS_PUSHBUTTON as u32, 392, 8, 28, 24, IDC_BTN_CLOSE);

        state.chk_case = mk("BUTTON", "区分大小写", BS_AUTOCHECKBOX as u32, 8, 38, 92, 22, IDC_CHK_CASE);
        state.chk_word = mk("BUTTON", "全词匹配", BS_AUTOCHECKBOX as u32, 106, 38, 80, 22, IDC_CHK_WORD);
        state.chk_regex = mk("BUTTON", "正则(Regex)", BS_AUTOCHECKBOX as u32, 192, 38, 100, 22, IDC_CHK_REGEX);

        state.replace_edit = mk("EDIT", "", WS_BORDER | ES_AUTOHSCROLL as u32, 8, 70, 200, 24, IDC_REPLACE_EDIT);
        mk("BUTTON", "替换", BS_PUSHBUTTON as u32, 214, 70, 68, 24, IDC_BTN_REPLACE);
        mk("BUTTON", "全部替换", BS_PUSHBUTTON as u32, 288, 70, 86, 24, IDC_BTN_REPLACE_ALL);

        state.orig_find_edit_proc = SetWindowLongPtrW(state.find_edit, GWLP_WNDPROC, edit_box_subclass as isize);
        state.orig_rep_edit_proc = SetWindowLongPtrW(state.replace_edit, GWLP_WNDPROC, edit_box_subclass as isize);
    }
}

unsafe extern "system" fn goto_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let main_hwnd = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HWND;
    let state_ptr = if !main_hwnd.is_null() {
        GetWindowLongPtrW(main_hwnd, GWLP_USERDATA) as *mut AppState
    } else {
        null_mut()
    };

    match msg {
        WM_COMMAND if !state_ptr.is_null() => {
            let st = &mut *state_ptr;
            let id = loword(wp) as i32;
            if id == IDC_GOTO_OK {
                let s = get_win_text(st.goto_edit);
                if let Ok(line_num) = s.trim().parse::<usize>() {
                    let target = line_num.saturating_sub(1).min(st.doc.line_count().saturating_sub(1));
                    let pos = st.doc.line_col_to_byte(target, 0);
                    st.doc.cursors = vec![Cursor::collapsed(pos)];
                    st.ensure_caret_visible();
                    st.invalidate();
                }
                DestroyWindow(hwnd);
                st.goto_hwnd = null_mut();
                SetFocus(st.hwnd);
                return 0;
            } else if id == IDC_GOTO_CANCEL {
                DestroyWindow(hwnd);
                st.goto_hwnd = null_mut();
                SetFocus(st.hwnd);
                return 0;
            }
        }
        WM_CLOSE if !state_ptr.is_null() => {
            let st = &mut *state_ptr;
            DestroyWindow(hwnd);
            st.goto_hwnd = null_mut();
            SetFocus(st.hwnd);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn show_goto_dialog(state: &mut AppState) {
    unsafe {
        if !state.goto_hwnd.is_null() {
            SetFocus(state.goto_edit);
            return;
        }
        let hinst = GetModuleHandleW(null());
        let cls = to_wide("NotepadLiteGotoDlg");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(goto_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszMenuName: null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc);

        let mut rc: RECT = std::mem::zeroed();
        GetWindowRect(state.hwnd, &mut rc);
        let title = to_wide("转到指定行");
        let dlg = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            cls.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            rc.left + 120,
            rc.top + 120,
            260,
            120,
            state.hwnd,
            null_mut(),
            hinst,
            null(),
        );
        SetWindowLongPtrW(dlg, GWLP_USERDATA, state.hwnd as isize);
        state.goto_hwnd = dlg;

        let lbl = to_wide(&format!("行号 (1 - {}):", state.doc.line_count()));
        let st_cls = to_wide("STATIC");
        let h1 = CreateWindowExW(0, st_cls.as_ptr(), lbl.as_ptr(), WS_CHILD | WS_VISIBLE, 12, 10, 220, 18, dlg, null_mut(), hinst, null());
        SendMessageW(h1, WM_SETFONT, state.hfont_ui as usize, 1);

        let ed_cls = to_wide("EDIT");
        let (cur_l, _) = state.doc.byte_to_line_col(state.doc.primary().head);
        let init_v = to_wide(&(cur_l + 1).to_string());
        let ed = CreateWindowExW(0, ed_cls.as_ptr(), init_v.as_ptr(), WS_CHILD | WS_VISIBLE | WS_BORDER | ES_NUMBER as u32, 12, 30, 220, 24, dlg, IDC_GOTO_EDIT as isize as HMENU, hinst, null());
        SendMessageW(ed, WM_SETFONT, state.hfont_ui as usize, 1);
        state.goto_edit = ed;

        let bt_cls = to_wide("BUTTON");
        let ok_t = to_wide("确定");
        let ca_t = to_wide("取消");
        let b1 = CreateWindowExW(0, bt_cls.as_ptr(), ok_t.as_ptr(), WS_CHILD | WS_VISIBLE | BS_DEFPUSHBUTTON as u32, 82, 60, 70, 24, dlg, IDC_GOTO_OK as isize as HMENU, hinst, null());
        let b2 = CreateWindowExW(0, bt_cls.as_ptr(), ca_t.as_ptr(), WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32, 162, 60, 70, 24, dlg, IDC_GOTO_CANCEL as isize as HMENU, hinst, null());
        SendMessageW(b1, WM_SETFONT, state.hfont_ui as usize, 1);
        SendMessageW(b2, WM_SETFONT, state.hfont_ui as usize, 1);
        SetFocus(ed);
        SendMessageW(ed, EM_SETSEL, 0, -1);
    }
}

fn handle_command(state: &mut AppState, id: usize) {
    match id {
        IDM_FILE_NEW => {
            if state.confirm_discard() {
                state.doc.set_content(String::new(), None, DocEncoding::Utf8, LineEnding::Crlf, 0);
                state.lang = Lang::Plain;
                state.scroll_y = 0;
                state.scroll_x = 0;
                state.update_gutter_width();
                state.rebuild_visual_lines();
                state.update_scrollbars();
                state.update_title();
                state.invalidate();
            }
        }
        IDM_FILE_OPEN => {
            if state.confirm_discard() {
                if let Some(p) = dialog_open_file(state.hwnd) {
                    state.open_file_path(&p, None);
                }
            }
        }
        IDM_FILE_SAVE => {
            state.do_save(false);
        }
        IDM_FILE_SAVEAS => {
            state.do_save(true);
        }
        IDM_FILE_RELOAD => {
            if let Some(p) = state.doc.path.clone() {
                if state.confirm_discard() {
                    state.open_file_path(&p, None);
                }
            }
        }
        IDM_FILE_EXIT => unsafe {
            PostMessageW(state.hwnd, WM_CLOSE, 0, 0);
        },

        IDM_EDIT_UNDO => {
            if state.doc.undo() {
                state.update_gutter_width();
                state.rebuild_visual_lines();
                state.ensure_caret_visible();
                state.update_title();
                state.invalidate();
            }
        }
        IDM_EDIT_REDO => {
            if state.doc.redo() {
                state.update_gutter_width();
                state.rebuild_visual_lines();
                state.ensure_caret_visible();
                state.update_title();
                state.invalidate();
            }
        }
        IDM_EDIT_COPY => {
            let mut txt = state.doc.selected_text_joined();
            if txt.is_empty() {
                let l = state.doc.line_of_byte(state.doc.primary().head);
                txt = state.doc.line_str(l).to_string();
            }
            clipboard_copy(state.hwnd, &txt);
        }
        IDM_EDIT_CUT => {
            let txt = state.doc.selected_text_joined();
            if !txt.is_empty() {
                clipboard_copy(state.hwnd, &txt);
                state.doc.insert_text("");
            } else {
                let l = state.doc.line_of_byte(state.doc.primary().head);
                clipboard_copy(state.hwnd, state.doc.line_str(l));
                state.doc.delete_current_line();
            }
            state.update_gutter_width();
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_PASTE => {
            if let Some(txt) = clipboard_paste(state.hwnd) {
                state.doc.insert_text(&txt);
                state.update_gutter_width();
                state.rebuild_visual_lines();
                state.ensure_caret_visible();
                state.update_title();
                state.invalidate();
            }
        }
        IDM_EDIT_SELECT_ALL => {
            state.doc.cursors = vec![Cursor { anchor: 0, head: state.doc.text.len() }];
            state.invalidate();
        }
        IDM_EDIT_DUP_LINE => {
            state.doc.duplicate_current_line();
            state.update_gutter_width();
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_DEL_LINE => {
            state.doc.delete_current_line();
            state.update_gutter_width();
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_MOVE_UP => {
            state.doc.move_lines(true);
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_MOVE_DOWN => {
            state.doc.move_lines(false);
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_COMMENT => {
            state.doc.toggle_comment(state.lang);
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_UPPER | IDM_EDIT_LOWER => {
            state.doc.transform_case(id == IDM_EDIT_UPPER);
            state.update_title();
            state.invalidate();
        }

        IDM_SEARCH_FIND => state.show_find_panel(false),
        IDM_SEARCH_REPLACE => state.show_find_panel(true),
        IDM_SEARCH_NEXT | IDM_SEARCH_PREV => {
            if !state.search.query.is_empty() {
                if state.search.matches.is_empty() {
                    state.refresh_search_ui();
                }
                if let Some((ms, me)) = state.search.step(id == IDM_SEARCH_NEXT) {
                    state.doc.cursors = vec![Cursor { anchor: ms, head: me }];
                    state.ensure_caret_visible();
                    state.invalidate();
                }
            } else {
                state.show_find_panel(false);
            }
        }
        IDM_SEARCH_GOTO => show_goto_dialog(state),

        IDM_VIEW_WRAP => {
            state.cfg.word_wrap = !state.cfg.word_wrap;
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_LINENUM => {
            state.cfg.show_line_numbers = !state.cfg.show_line_numbers;
            state.update_gutter_width();
            state.rebuild_visual_lines();
            state.update_scrollbars();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_WHITESPACE => {
            state.cfg.show_whitespace = !state.cfg.show_whitespace;
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_FONT => dialog_choose_font(state),
        IDM_VIEW_THEME => {
            state.cfg.dark_theme = !state.cfg.dark_theme;
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_TOPMOST => {
            state.cfg.always_on_top = !state.cfg.always_on_top;
            let insert_after = if state.cfg.always_on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
            unsafe {
                SetWindowPos(state.hwnd, insert_after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
            save_config(&state.cfg);
        }
        IDM_VIEW_ZOOM_IN => {
            state.cfg.zoom_pct = (state.cfg.zoom_pct + 10).min(300);
            state.recreate_fonts();
            state.ensure_caret_visible();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_ZOOM_OUT => {
            state.cfg.zoom_pct = (state.cfg.zoom_pct - 10).max(50);
            state.recreate_fonts();
            state.ensure_caret_visible();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_ZOOM_RESET => {
            state.cfg.zoom_pct = 100;
            state.recreate_fonts();
            state.ensure_caret_visible();
            save_config(&state.cfg);
            state.invalidate();
        }

        IDM_ENC_REOPEN_UTF8..=IDM_ENC_REOPEN_BIG5 => {
            let enc = match id {
                IDM_ENC_REOPEN_UTF8 => DocEncoding::Utf8,
                IDM_ENC_REOPEN_UTF8BOM => DocEncoding::Utf8Bom,
                IDM_ENC_REOPEN_UTF16LE => DocEncoding::Utf16Le,
                IDM_ENC_REOPEN_UTF16BE => DocEncoding::Utf16Be,
                IDM_ENC_REOPEN_GBK => DocEncoding::Gbk,
                _ => DocEncoding::Big5,
            };
            if let Some(p) = state.doc.path.clone() {
                if state.confirm_discard() {
                    state.open_file_path(&p, Some(enc));
                }
            }
        }
        IDM_ENC_CONV_UTF8..=IDM_ENC_CONV_BIG5 => {
            state.doc.encoding = match id {
                IDM_ENC_CONV_UTF8 => DocEncoding::Utf8,
                IDM_ENC_CONV_UTF8BOM => DocEncoding::Utf8Bom,
                IDM_ENC_CONV_UTF16LE => DocEncoding::Utf16Le,
                IDM_ENC_CONV_UTF16BE => DocEncoding::Utf16Be,
                IDM_ENC_CONV_GBK => DocEncoding::Gbk,
                _ => DocEncoding::Big5,
            };
            state.doc.dirty = true;
            state.update_title();
            state.invalidate();
        }
        IDM_EOL_CRLF => {
            state.doc.line_ending = LineEnding::Crlf;
            state.doc.dirty = true;
            state.update_title();
            state.invalidate();
        }
        IDM_EOL_LF => {
            state.doc.line_ending = LineEnding::Lf;
            state.doc.dirty = true;
            state.update_title();
            state.invalidate();
        }
        _ => {}
    }
}

fn handle_keydown(state: &mut AppState, vk: u16) -> bool {
    let ctrl = key_down(VK_CONTROL);
    let shift = key_down(VK_SHIFT);
    let alt = key_down(VK_MENU);

    if ctrl && !alt {
        match vk as i32 {
            0x4E => { handle_command(state, IDM_FILE_NEW); return true; } // N
            0x4F => { handle_command(state, IDM_FILE_OPEN); return true; } // O
            0x53 => { handle_command(state, if shift { IDM_FILE_SAVEAS } else { IDM_FILE_SAVE }); return true; } // S
            0x5A => { handle_command(state, IDM_EDIT_UNDO); return true; } // Z
            0x59 => { handle_command(state, IDM_EDIT_REDO); return true; } // Y
            0x58 => { handle_command(state, IDM_EDIT_CUT); return true; } // X
            0x43 => { handle_command(state, IDM_EDIT_COPY); return true; } // C
            0x56 => { handle_command(state, IDM_EDIT_PASTE); return true; } // V
            0x41 => { handle_command(state, IDM_EDIT_SELECT_ALL); return true; } // A
            0x44 => { handle_command(state, IDM_EDIT_DUP_LINE); return true; } // D
            0x4B if shift => { handle_command(state, IDM_EDIT_DEL_LINE); return true; } // K
            0x55 => { handle_command(state, if shift { IDM_EDIT_UPPER } else { IDM_EDIT_LOWER }); return true; } // U
            0x46 => { handle_command(state, IDM_SEARCH_FIND); return true; } // F
            0x48 => { handle_command(state, IDM_SEARCH_REPLACE); return true; } // H
            0x47 => { handle_command(state, IDM_SEARCH_GOTO); return true; } // G
            0xBF => { handle_command(state, IDM_EDIT_COMMENT); return true; } // /
            0xBB => { handle_command(state, IDM_VIEW_ZOOM_IN); return true; } // +
            0xBD => { handle_command(state, IDM_VIEW_ZOOM_OUT); return true; } // -
            0x30 => { handle_command(state, IDM_VIEW_ZOOM_RESET); return true; } // 0
            _ => {}
        }
    }
    if alt && !ctrl {
        match vk {
            VK_UP => { handle_command(state, IDM_EDIT_MOVE_UP); return true; }
            VK_DOWN => { handle_command(state, IDM_EDIT_MOVE_DOWN); return true; }
            0x5A => { handle_command(state, IDM_VIEW_WRAP); return true; } // Alt+Z
            _ => {}
        }
    }
    if vk == VK_F3 {
        handle_command(state, if shift { IDM_SEARCH_PREV } else { IDM_SEARCH_NEXT });
        return true;
    }
    if vk == VK_F5 {
        handle_command(state, IDM_FILE_RELOAD);
        return true;
    }
    if vk == VK_ESCAPE {
        if state.search.visible {
            state.hide_find_panel();
            return true;
        }
        if state.doc.cursors.len() > 1 {
            state.doc.cursors.truncate(1);
            state.invalidate();
            return true;
        }
    }

    // 导航与基础编辑键
    let move_cursor = |st: &mut AppState, next_pos: usize| {
        let head = st.doc.clamp_char_boundary(next_pos);
        if shift {
            st.doc.cursors[0].head = head;
        } else {
            st.doc.cursors = vec![Cursor::collapsed(head)];
        }
        st.ensure_caret_visible();
        st.invalidate();
    };

    let head = state.doc.primary().head;
    let (line, col) = state.doc.byte_to_line_col(head);
    match vk {
        VK_LEFT => {
            if !shift && state.doc.primary().has_selection() {
                let (s, _) = state.doc.primary().range();
                move_cursor(state, s);
            } else {
                move_cursor(state, state.doc.prev_char_boundary(head));
            }
            true
        }
        VK_RIGHT => {
            if !shift && state.doc.primary().has_selection() {
                let (_, e) = state.doc.primary().range();
                move_cursor(state, e);
            } else {
                move_cursor(state, state.doc.next_char_boundary(head));
            }
            true
        }
        VK_UP => {
            let nl = line.saturating_sub(1);
            move_cursor(state, state.doc.line_col_to_byte(nl, col));
            true
        }
        VK_DOWN => {
            let nl = (line + 1).min(state.doc.line_count().saturating_sub(1));
            move_cursor(state, state.doc.line_col_to_byte(nl, col));
            true
        }
        VK_HOME => {
            let target = if ctrl { 0 } else { state.doc.line_bounds(line).0 };
            move_cursor(state, target);
            true
        }
        VK_END => {
            let target = if ctrl { state.doc.text.len() } else { state.doc.line_bounds(line).1 };
            move_cursor(state, target);
            true
        }
        VK_PRIOR => {
            let nl = line.saturating_sub(state.visible_row_count());
            move_cursor(state, state.doc.line_col_to_byte(nl, col));
            true
        }
        VK_NEXT => {
            let nl = (line + state.visible_row_count()).min(state.doc.line_count().saturating_sub(1));
            move_cursor(state, state.doc.line_col_to_byte(nl, col));
            true
        }
        VK_BACK => {
            state.doc.backspace();
            state.update_gutter_width();
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
            true
        }
        VK_DELETE => {
            state.doc.delete_forward();
            state.update_gutter_width();
            state.rebuild_visual_lines();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
            true
        }
        _ => false,
    }
}

// ============================================================================
// 12. Win32 主窗口过程与程序入口
// ============================================================================

unsafe extern "system" fn main_wnd_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let st = &mut *ptr;

    match msg {
        WM_SIZE => {
            st.client_w = loword(lp as usize) as i32;
            st.client_h = hiword(lp as usize) as i32;
            st.rebuild_visual_lines();
            st.update_scrollbars();
            if st.search.visible {
                st.layout_find_panel();
            }
            st.invalidate();
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_editor(st, hdc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_SETCURSOR => {
            if loword(lp as usize) as u32 == HTCLIENT {
                let mut pt: POINT = std::mem::zeroed();
                GetCursorPos(&mut pt);
                ScreenToClient(hwnd, &mut pt);
                if pt.x >= st.gutter_w && pt.y < st.client_h - STATUS_BAR_HEIGHT {
                    SetCursor(LoadCursorW(null_mut(), IDC_IBEAM));
                } else {
                    SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
                }
                return 1;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_INITMENUPOPUP => {
            sync_menu_checks(st, GetMenu(hwnd));
            0
        }
        WM_COMMAND => {
            handle_command(st, loword(wp) as usize);
            0
        }
        WM_KEYDOWN => {
            if handle_keydown(st, wp as u16) {
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_SYSKEYDOWN => {
            if key_down(VK_MENU) && !key_down(VK_CONTROL) {
                if wp == VK_UP as usize || wp == VK_DOWN as usize || wp == 0x5A {
                    if handle_keydown(st, wp as u16) {
                        return 0;
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CHAR => {
            if key_down(VK_CONTROL) {
                return 0;
            }
            let code = wp as u32;
            if code == 0x0D {
                st.doc.insert_text("\n");
            } else if code == 0x09 {
                st.doc.insert_text("    ");
            } else if code >= 0x20 {
                if let Some(ch) = char::from_u32(code) {
                    let mut buf = [0u8; 4];
                    st.doc.insert_text(ch.encode_utf8(&mut buf));
                }
            } else {
                return 0;
            }
            st.update_gutter_width();
            st.rebuild_visual_lines();
            st.ensure_caret_visible();
            st.update_title();
            st.invalidate();
            0
        }
        WM_IME_STARTCOMPOSITION => {
            st.update_ime_pos();
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN => {
            SetFocus(hwnd);
            SetCapture(hwnd);
            st.dragging = true;
            let (pos, line, col) = st.hit_test(get_x_lparam(lp), get_y_lparam(lp));
            if key_down(VK_MENU) {
                st.rect_selecting = true;
                st.rect_anchor = (line, col);
                st.doc.cursors = vec![Cursor::collapsed(pos)];
            } else if key_down(VK_CONTROL) {
                st.rect_selecting = false;
                st.doc.cursors.push(Cursor::collapsed(pos));
                st.doc.normalize_cursors();
            } else if key_down(VK_SHIFT) {
                st.rect_selecting = false;
                st.doc.cursors[0].head = pos;
            } else {
                st.rect_selecting = false;
                st.doc.cursors = vec![Cursor::collapsed(pos)];
            }
            st.invalidate();
            0
        }
        WM_MOUSEMOVE => {
            if st.dragging {
                let (pos, line, col) = st.hit_test(get_x_lparam(lp), get_y_lparam(lp));
                if st.rect_selecting {
                    let (l1, l2) = (st.rect_anchor.0.min(line), st.rect_anchor.0.max(line));
                    let (c1, c2) = (st.rect_anchor.1.min(col), st.rect_anchor.1.max(col));
                    let mut rect_cursors = Vec::new();
                    for l in l1..=l2 {
                        let a = st.doc.line_col_to_byte(l, c1);
                        let h = st.doc.line_col_to_byte(l, c2);
                        rect_cursors.push(Cursor { anchor: a, head: h });
                    }
                    if !rect_cursors.is_empty() {
                        st.doc.cursors = rect_cursors;
                    }
                } else {
                    if let Some(last) = st.doc.cursors.last_mut() {
                        last.head = pos;
                    }
                }
                st.invalidate();
            }
            0
        }
        WM_LBUTTONUP => {
            if st.dragging {
                st.dragging = false;
                st.rect_selecting = false;
                ReleaseCapture();
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp >> 16) as i16) as i32;
            if key_down(VK_CONTROL) {
                handle_command(st, if delta > 0 { IDM_VIEW_ZOOM_IN } else { IDM_VIEW_ZOOM_OUT });
            } else {
                let step = if delta > 0 { -3isize } else { 3isize };
                let max_y = st.total_visual_lines().saturating_sub(1) as isize;
                st.scroll_y = (st.scroll_y as isize + step).clamp(0, max_y) as usize;
                st.update_scrollbars();
                st.invalidate();
            }
            0
        }
        WM_VSCROLL => {
            let code = loword(wp) as u32;
            let mut si: SCROLLINFO = std::mem::zeroed();
            si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
            si.fMask = SIF_ALL;
            GetScrollInfo(hwnd, SB_VERT, &mut si);
            let mut pos = si.nPos;
            match code {
                SB_LINEUP => pos -= 1,
                SB_LINEDOWN => pos += 1,
                SB_PAGEUP => pos -= si.nPage as i32,
                SB_PAGEDOWN => pos += si.nPage as i32,
                SB_THUMBTRACK => pos = si.nTrackPos,
                _ => {}
            }
            let max_pos = (si.nMax - (si.nPage as i32) + 1).max(0);
            st.scroll_y = pos.clamp(0, max_pos) as usize;
            st.update_scrollbars();
            st.invalidate();
            0
        }
        WM_HSCROLL => {
            let code = loword(wp) as u32;
            let mut si: SCROLLINFO = std::mem::zeroed();
            si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
            si.fMask = SIF_ALL;
            GetScrollInfo(hwnd, SB_HORZ, &mut si);
            let mut pos = si.nPos;
            match code {
                SB_LINELEFT => pos -= st.avg_char_w * 2,
                SB_LINERIGHT => pos += st.avg_char_w * 2,
                SB_PAGELEFT => pos -= si.nPage as i32,
                SB_PAGERIGHT => pos += si.nPage as i32,
                SB_THUMBTRACK => pos = si.nTrackPos,
                _ => {}
            }
            st.scroll_x = pos.clamp(0, si.nMax);
            st.update_scrollbars();
            st.invalidate();
            0
        }
        WM_DROPFILES => {
            let hdrop = wp as HDROP;
            let mut buf = vec![0u16; 2048];
            let len = DragQueryFileW(hdrop, 0, buf.as_mut_ptr(), buf.len() as u32);
            DragFinish(hdrop);
            if len > 0 && st.confirm_discard() {
                let p = PathBuf::from(from_wide_slice(&buf));
                st.open_file_path(&p, None);
            }
            0
        }
        WM_CLOSE => {
            if !st.confirm_discard() {
                return 0;
            }
            let mut wp_info: WINDOWPLACEMENT = std::mem::zeroed();
            wp_info.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(hwnd, &mut wp_info) != 0 {
                st.cfg.maximized = wp_info.showCmd == SW_SHOWMAXIMIZED as u32;
                st.cfg.win_x = wp_info.rcNormalPosition.left;
                st.cfg.win_y = wp_info.rcNormalPosition.top;
                st.cfg.win_w = (wp_info.rcNormalPosition.right - wp_info.rcNormalPosition.left).max(400);
                st.cfg.win_h = (wp_info.rcNormalPosition.bottom - wp_info.rcNormalPosition.top).max(300);
            }
            save_config(&st.cfg);
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() {
    // 第 1 行立即启动日志系统与崩溃防护
    init_logger();

    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        let cfg = load_config();

        let cls_name = to_wide("NotepadLiteMainWindow");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(main_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: cls_name.as_ptr(),
            hIconSm: null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            let err = format!("RegisterClassExW failed: {}", GetLastError());
            log_msg("FATAL", &err);
            show_fatal_box("初始化窗口类失败", &err);
            return;
        }

        let hmenu = build_main_menu();
        let ex_style = if cfg.always_on_top { WS_EX_TOPMOST } else { 0 };
        let title = to_wide("无标题 - NotepadLite");
        let hwnd = CreateWindowExW(
            ex_style,
            cls_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VSCROLL | WS_HSCROLL | WS_CLIPCHILDREN,
            cfg.win_x,
            cfg.win_y,
            cfg.win_w,
            cfg.win_h,
            null_mut(),
            hmenu,
            hinst,
            null(),
        );
        if hwnd.is_null() {
            let err = format!("CreateWindowExW failed: {}", GetLastError());
            log_msg("FATAL", &err);
            show_fatal_box("创建主窗口失败", &err);
            return;
        }

        let ui_font_name = to_wide("Microsoft YaHei UI");
        let hfont_ui = CreateFontW(
            -13, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0,
            DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, ui_font_name.as_ptr(),
        );

        let mut state = Box::new(AppState {
            hwnd,
            hfont_editor: null_mut(),
            hfont_ui,
            doc: Document::new_empty(),
            cfg,
            search: SearchState::new(),
            lang: Lang::Plain,
            client_w: 1000,
            client_h: 700,
            line_h: 20,
            avg_char_w: 9,
            gutter_w: 44,
            scroll_y: 0,
            scroll_x: 0,
            visual_lines: Vec::new(),
            dragging: false,
            rect_selecting: false,
            rect_anchor: (0, 0),
            find_panel: null_mut(),
            find_edit: null_mut(),
            replace_edit: null_mut(),
            chk_case: null_mut(),
            chk_word: null_mut(),
            chk_regex: null_mut(),
            lbl_count: null_mut(),
            orig_find_edit_proc: 0,
            orig_rep_edit_proc: 0,
            goto_hwnd: null_mut(),
            goto_edit: null_mut(),
        });

        state.recreate_fonts();
        create_find_panel(&mut state, hinst);

        let state_ptr: *mut AppState = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        DragAcceptFiles(hwnd, 1);

        // 读取命令行参数启动文件
        if let Some(arg_path) = std::env::args_os().nth(1) {
            let p = PathBuf::from(arg_path);
            if p.exists() {
                (*state_ptr).open_file_path(&p, None);
            }
        }

        let show_cmd = if (*state_ptr).cfg.maximized { SW_SHOWMAXIMIZED } else { SW_SHOW };
        ShowWindow(hwnd, show_cmd);
        UpdateWindow(hwnd);
        log_msg("INFO", "Main window ready");

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = Box::from_raw(state_ptr);
        log_msg("INFO", "=== NotepadLite exited cleanly ===");
    }
}
