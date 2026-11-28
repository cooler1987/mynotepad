use chrono::Local;
use encoding_rs::{Encoding, BIG5, GB18030, UTF_16BE, UTF_16LE, UTF_8};
use regex_lite::{Captures, Regex, RegexBuilder};
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

pub const LARGE_FILE_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_MATCH_HIGHLIGHTS: usize = 50_000;
const CURRENT_CONFIG_VERSION: u32 = 2;
const INLINE_LOOKAHEAD: usize = 512;

pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn from_wide_slice(slice: &[u16]) -> String {
    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..end])
}

#[inline]
pub fn floor_char_boundary(s: &str, pos: usize) -> usize {
    let mut p = pos.min(s.len());
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

// ============================================================================
// 1. 启动期错误日志与崩溃捕获
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
// 2. 绿色便携配置（exe 同级目录 config.json）
// ============================================================================

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub config_version: u32,
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
    pub outline_visible: bool,
    pub outline_width: i32,
    pub outline_floating: bool,
    pub outline_x: i32,
    pub outline_y: i32,
    pub outline_float_w: i32,
    pub outline_float_h: i32,
    pub recent_files: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
            win_x: CW_USEDEFAULT,
            win_y: CW_USEDEFAULT,
            win_w: 1120,
            win_h: 740,
            maximized: false,
            font_name: "Microsoft YaHei UI".to_string(),
            font_size: 16,
            zoom_pct: 100,
            dark_theme: false,
            word_wrap: true,
            show_line_numbers: false,
            show_whitespace: false,
            always_on_top: false,
            outline_visible: false,
            outline_width: 245,
            outline_floating: false,
            outline_x: CW_USEDEFAULT,
            outline_y: CW_USEDEFAULT,
            outline_float_w: 270,
            outline_float_h: 440,
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
        Ok(s) => {
            let mut cfg = serde_json::from_str::<AppConfig>(&s).unwrap_or_else(|e| {
                log_msg("WARN", &format!("config.json parse error, using default: {}", e));
                AppConfig::default()
            });
            if cfg.config_version < CURRENT_CONFIG_VERSION {
                cfg.config_version = CURRENT_CONFIG_VERSION;
                cfg.word_wrap = true;
                cfg.show_line_numbers = false;
                cfg.outline_visible = false;
                save_config(&cfg);
            }
            cfg
        }
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
// 3. 字符编码自动识别与读写
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

pub fn load_file_auto_encoding(path: &Path) -> Result<(String, DocEncoding, LineEnding, usize), String> {
    let bytes = fs::read(path).map_err(|e| format!("读取文件失败: {}", e))?;
    let byte_size = bytes.len();

    let (doc_enc, bom_len, rs_enc): (DocEncoding, usize, &'static Encoding) =
        if let Some((enc, bl)) = Encoding::for_bom(&bytes) {
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
            let sample = bytes.len().min(64 * 1024);
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
// 4. 高性能文本缓冲、标题/代码块折叠、MD编辑操作与撤销重做引擎
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

#[derive(Clone, Copy)]
pub struct CodeBlockRange {
    pub start_line: usize,
    pub end_line: usize,
    pub has_closing_fence: bool,
    pub inner_start_byte: usize,
    pub inner_end_byte: usize,
    pub block_start_byte: usize,
    pub block_end_byte: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MdBlockStyle {
    Paragraph,
    Heading(u8),
    BulletList,
    OrderedList,
    TaskList,
    Quote,
    CodeBlock,
}

fn detect_fold_level(line: &str, state: LineState) -> u8 {
    if !matches!(state, LineState::Normal) {
        return 0;
    }
    if parse_fence_open(line).is_some() {
        return 7; // 7 表示围栏代码块起始行（可独立折叠）
    }
    let trimmed = line.trim_start();
    let ws = line.len() - trimmed.len();
    if ws <= 3 && trimmed.starts_with('#') {
        let mut h = 0usize;
        while h < trimmed.len() && trimmed.as_bytes()[h] == b'#' {
            h += 1;
        }
        if (1..=6).contains(&h)
            && (h == trimmed.len() || trimmed.as_bytes()[h] == b' ' || trimmed.as_bytes()[h] == b'\t')
        {
            return h as u8;
        }
    }
    0
}

fn strip_md_block_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    let b = trimmed.as_bytes();
    if b.is_empty() {
        return "";
    }
    // 剥离标题 #..######
    if b[0] == b'#' {
        let mut h = 0;
        while h < b.len() && b[h] == b'#' {
            h += 1;
        }
        if (1..=6).contains(&h) && (h == b.len() || b[h] == b' ' || b[h] == b'\t') {
            return trimmed[h..].trim_start();
        }
    }
    // 剥离引用 >
    if b[0] == b'>' {
        let rest = &trimmed[1..];
        return rest.strip_prefix(' ').unwrap_or(rest);
    }
    // 剥离任务列表与无序列表
    if matches!(b[0], b'-' | b'*' | b'+') && b.get(1).copied().is_some_and(|c| c == b' ' || c == b'\t') {
        let after = trimmed[2..].trim_start();
        if after.starts_with("[ ] ") || after.starts_with("[x] ") || after.starts_with("[X] ") {
            return &after[4..];
        }
        if after == "[ ]" || after == "[x]" || after == "[X]" {
            return "";
        }
        return after;
    }
    // 剥离有序列表 1. / 1)
    let mut d = 0usize;
    while d < b.len() && b[d].is_ascii_digit() && d < 9 {
        d += 1;
    }
    if d > 0 && d + 1 < b.len() && (b[d] == b'.' || b[d] == b')') && (b[d + 1] == b' ' || b[d + 1] == b'\t') {
        return trimmed[d + 2..].trim_start();
    }
    trimmed
}

#[derive(Clone, Copy)]
enum ActiveFold {
    Heading(u8),
    CodeBlock,
}

pub struct Document {
    pub text: String,
    pub line_starts: Vec<usize>,
    pub line_states: Vec<LineState>,
    pub line_levels: Vec<u8>,   // 0=普通行, 1..=6=MD标题级别, 7=代码块起始行
    pub line_folded: Vec<bool>, // 某标题或代码块行是否已折叠
    pub line_hidden: Vec<bool>, // 被上级折叠隐藏的行
    pub wrap_patches: Vec<(usize, usize, usize)>,
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
    pub lang: Lang,
}

impl Document {
    pub fn new_empty() -> Self {
        Self {
            text: String::new(),
            line_starts: vec![0],
            line_states: vec![LineState::Normal],
            line_levels: vec![0],
            line_folded: vec![false],
            line_hidden: vec![false],
            wrap_patches: Vec::new(),
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
            lang: Lang::Markdown,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn has_any_selection(&self) -> bool {
        self.cursors.iter().any(|c| c.has_selection())
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
        let count = self.line_starts.len();
        self.line_folded.clear();
        self.line_folded.resize(count, false);
        self.line_hidden.clear();
        self.line_hidden.resize(count, false);
        self.rebuild_all_line_states();
    }

    pub fn rebuild_all_line_states(&mut self) {
        let count = self.line_starts.len();
        self.line_states.clear();
        self.line_states.resize(count, LineState::Normal);
        self.line_levels.clear();
        self.line_levels.resize(count, 0);
        if self.lang != Lang::Markdown {
            return;
        }
        let mut st = LineState::Normal;
        for l in 0..count {
            self.line_states[l] = st;
            let (lvl, next_st) = {
                let line = self.line_str(l);
                (detect_fold_level(line, st), advance_md_line_state(st, line))
            };
            self.line_levels[l] = lvl;
            st = next_st;
        }
    }

    fn update_line_states_incremental(&mut self, start_line: usize, dirty_end_line: usize) {
        if self.lang != Lang::Markdown {
            return;
        }
        let count = self.line_starts.len();
        if count == 0 || start_line >= count {
            return;
        }
        let mut st = self.line_states[start_line];
        for l in start_line..count {
            self.line_states[l] = st;
            let (lvl, next_st) = {
                let line = self.line_str(l);
                (detect_fold_level(line, st), advance_md_line_state(st, line))
            };
            self.line_levels[l] = lvl;
            st = next_st;
            if l + 1 < count && l >= dirty_end_line && self.line_states[l + 1] == st {
                break;
            }
        }
    }

    #[inline]
    fn is_closing_fence_line(&self, line: usize) -> bool {
        let count = self.line_count();
        matches!(self.line_states.get(line), Some(LineState::FencedCode { .. }))
            && (line + 1 >= count || matches!(self.line_states.get(line + 1), Some(LineState::Normal)))
    }

    pub fn recompute_hidden_lines(&mut self) {
        let count = self.line_starts.len();
        if self.line_hidden.len() != count {
            self.line_hidden.resize(count, false);
        }
        let mut active: Option<ActiveFold> = None;
        for l in 0..count {
            let lvl = self.line_levels.get(l).copied().unwrap_or(0);
            match active {
                Some(ActiveFold::Heading(fl)) => {
                    if (1..=6).contains(&lvl) && lvl <= fl {
                        active = None;
                    } else {
                        self.line_hidden[l] = true;
                        continue;
                    }
                }
                Some(ActiveFold::CodeBlock) => {
                    self.line_hidden[l] = true;
                    if self.is_closing_fence_line(l) {
                        active = None;
                    }
                    continue;
                }
                None => {}
            }
            self.line_hidden[l] = false;
            if self.line_folded.get(l).copied().unwrap_or(false) {
                if (1..=6).contains(&lvl) {
                    active = Some(ActiveFold::Heading(lvl));
                } else if lvl == 7 {
                    active = Some(ActiveFold::CodeBlock);
                }
            }
        }
    }

    pub fn toggle_fold_at_line(&mut self, line: usize) -> bool {
        if line >= self.line_folded.len() || self.line_levels.get(line).copied().unwrap_or(0) == 0 {
            return false;
        }
        self.line_folded[line] = !self.line_folded[line];
        self.recompute_hidden_lines();
        true
    }

    pub fn set_all_folds(&mut self, folded: bool) {
        for l in 0..self.line_folded.len() {
            let lvl = self.line_levels.get(l).copied().unwrap_or(0);
            if (1..=6).contains(&lvl) {
                self.line_folded[l] = folded;
            } else if !folded && lvl == 7 {
                self.line_folded[l] = false;
            } else if lvl == 0 {
                self.line_folded[l] = false;
            }
        }
        self.recompute_hidden_lines();
    }

    pub fn ensure_line_unfolded(&mut self, target_line: usize) -> bool {
        if target_line >= self.line_hidden.len() || !self.line_hidden[target_line] {
            return false;
        }
        let mut changed = false;
        if let Some(cb) = self.code_block_range_at(target_line) {
            if self.line_folded.get(cb.start_line).copied().unwrap_or(false) {
                self.line_folded[cb.start_line] = false;
                changed = true;
            }
        }
        let mut min_lvl = u8::MAX;
        for l in (0..target_line).rev() {
            let lvl = self.line_levels[l];
            if (1..=6).contains(&lvl) && lvl < min_lvl {
                if self.line_folded[l] {
                    self.line_folded[l] = false;
                    changed = true;
                }
                min_lvl = lvl;
                if min_lvl == 1 {
                    break;
                }
            }
        }
        if changed {
            self.recompute_hidden_lines();
        }
        changed
    }

    pub fn folded_descendant_count(&self, line: usize) -> usize {
        if !self.line_folded.get(line).copied().unwrap_or(false) {
            return 0;
        }
        let fl = self.line_levels.get(line).copied().unwrap_or(0);
        if fl == 0 {
            return 0;
        }
        let mut cnt = 0usize;
        if fl == 7 {
            for l in (line + 1)..self.line_count() {
                cnt += 1;
                if self.is_closing_fence_line(l) {
                    break;
                }
            }
            return cnt;
        }
        for l in (line + 1)..self.line_count() {
            let lvl = self.line_levels[l];
            if (1..=6).contains(&lvl) && lvl <= fl {
                break;
            }
            cnt += 1;
        }
        cnt
    }

    pub fn heading_for_line(&self, line: usize) -> Option<usize> {
        let mut l = line.min(self.line_count().saturating_sub(1));
        loop {
            let lvl = self.line_levels.get(l).copied().unwrap_or(0);
            if (1..=6).contains(&lvl) {
                return Some(l);
            }
            if l == 0 {
                return None;
            }
            l -= 1;
        }
    }

    pub fn code_block_range_at(&self, line: usize) -> Option<CodeBlockRange> {
        if self.lang != Lang::Markdown || line >= self.line_count() {
            return None;
        }
        let st = self.line_states.get(line).copied().unwrap_or(LineState::Normal);
        let start_line = match st {
            LineState::Normal => {
                if parse_fence_open(self.line_str(line)).is_some() {
                    line
                } else {
                    return None;
                }
            }
            LineState::FencedCode { .. } => {
                let mut l = line;
                loop {
                    if l == 0 {
                        break 0;
                    }
                    l -= 1;
                    if matches!(self.line_states[l], LineState::Normal) {
                        break l;
                    }
                }
            }
        };

        let count = self.line_count();
        let mut end_line = start_line;
        let mut has_closing_fence = false;
        for l in (start_line + 1)..count {
            end_line = l;
            if self.is_closing_fence_line(l) {
                has_closing_fence = !matches!(self.line_states.get(l + 1), Some(LineState::FencedCode { .. }))
                    && (l + 1 < count || matches!(advance_md_line_state(self.line_states[l], self.line_str(l)), LineState::Normal));
                break;
            }
        }

        let inner_start_byte = if start_line + 1 <= end_line && start_line + 1 < count {
            self.line_starts[start_line + 1]
        } else {
            self.line_bounds(start_line).1
        };
        let inner_end_byte = if has_closing_fence {
            if end_line > start_line + 1 {
                self.line_bounds(end_line - 1).1
            } else {
                inner_start_byte
            }
        } else if end_line > start_line {
            self.line_bounds(end_line).1
        } else {
            inner_start_byte
        };

        let bs = self.line_starts[start_line];
        let be = self.line_bounds(end_line).1;
        let (block_start_byte, block_end_byte) = if be < self.text.len() {
            (bs, be + 1)
        } else if bs > 0 {
            (bs - 1, be)
        } else {
            (0, be)
        };

        Some(CodeBlockRange {
            start_line,
            end_line,
            has_closing_fence,
            inner_start_byte,
            inner_end_byte: inner_end_byte.max(inner_start_byte),
            block_start_byte,
            block_end_byte,
        })
    }

    pub fn code_block_text(&self, cb: CodeBlockRange) -> String {
        if cb.inner_start_byte >= cb.inner_end_byte {
            return String::new();
        }
        self.text[cb.inner_start_byte..cb.inner_end_byte].replace('\n', "\r\n")
    }

    pub fn code_block_full_text(&self, cb: CodeBlockRange) -> String {
        let bs = self.line_starts[cb.start_line];
        let be = self.line_bounds(cb.end_line).1;
        self.text[bs..be].replace('\n', "\r\n")
    }

    pub fn delete_code_block(&mut self, cb: CodeBlockRange) {
        let target = cb.block_start_byte.min(self.text.len());
        self.apply_edits(
            vec![(cb.block_start_byte, cb.block_end_byte, String::new())],
            Some(vec![Cursor::collapsed(target)]),
        );
    }

    pub fn transform_block_style(&mut self, style: MdBlockStyle) {
        let (s, e) = self.primary().range();
        let l1 = self.line_of_byte(s);
        let l2 = self.line_of_byte(e);
        let b_start = self.line_bounds(l1).0;
        let b_end = self.line_bounds(l2).1;

        if style == MdBlockStyle::CodeBlock {
            let body = &self.text[b_start..b_end];
            let rep = format!("```\n{}\n```", body);
            let caret = b_start + 3;
            self.apply_edits(vec![(b_start, b_end, rep)], Some(vec![Cursor::collapsed(caret)]));
            return;
        }

        let toggle_back = if let MdBlockStyle::Heading(lvl) = style {
            (l1..=l2).all(|l| self.line_levels.get(l).copied().unwrap_or(0) == lvl)
        } else {
            false
        };

        let mut out_lines = Vec::with_capacity(l2 - l1 + 1);
        for (idx, l) in (l1..=l2).enumerate() {
            let line = self.line_str(l);
            let stripped = strip_md_block_prefix(line);
            let next = if toggle_back || style == MdBlockStyle::Paragraph {
                stripped.to_string()
            } else {
                match style {
                    MdBlockStyle::Heading(lvl) => format!("{} {}", "#".repeat(lvl as usize), stripped),
                    MdBlockStyle::BulletList => format!("- {}", stripped),
                    MdBlockStyle::OrderedList => format!("{}. {}", idx + 1, stripped),
                    MdBlockStyle::TaskList => format!("- [ ] {}", stripped),
                    MdBlockStyle::Quote => format!("> {}", stripped),
                    MdBlockStyle::Paragraph | MdBlockStyle::CodeBlock => stripped.to_string(),
                }
            };
            out_lines.push(next);
        }
        let rep = out_lines.join("\n");
        let caret = b_start + rep.len();
        self.apply_edits(vec![(b_start, b_end, rep)], Some(vec![Cursor::collapsed(caret)]));
    }

    pub fn indent_lines(&mut self, indent: bool) {
        let (s, e) = self.primary().range();
        let l1 = self.line_of_byte(s);
        let l2 = self.line_of_byte(e);
        let mut edits = Vec::new();
        for l in l1..=l2 {
            let (ls, _) = self.line_bounds(l);
            if indent {
                edits.push((ls, ls, "    ".to_string()));
            } else {
                let line = self.line_str(l);
                let lb = line.as_bytes();
                if lb.first() == Some(&b'\t') {
                    edits.push((ls, ls + 1, String::new()));
                } else {
                    let mut sp = 0usize;
                    while sp < 4 && lb.get(sp) == Some(&b' ') {
                        sp += 1;
                    }
                    if sp > 0 {
                        edits.push((ls, ls + sp, String::new()));
                    }
                }
            }
        }
        self.apply_edits(edits, None);
    }

    pub fn toggle_inline_wrap(&mut self, prefix: &str, suffix: &str, placeholder: &str) {
        let (s, e) = self.primary().range();
        if s < e {
            let sel = &self.text[s..e];
            if sel.starts_with(prefix) && sel.ends_with(suffix) && sel.len() >= prefix.len() + suffix.len() {
                let inner = sel[prefix.len()..sel.len() - suffix.len()].to_string();
                let ilen = inner.len();
                self.apply_edits(vec![(s, e, inner)], Some(vec![Cursor { anchor: s, head: s + ilen }]));
                return;
            }
            if s >= prefix.len()
                && e + suffix.len() <= self.text.len()
                && &self.text[s - prefix.len()..s] == prefix
                && &self.text[e..e + suffix.len()] == suffix
            {
                let inner = sel.to_string();
                let ilen = inner.len();
                let ns = s - prefix.len();
                self.apply_edits(
                    vec![(ns, e + suffix.len(), inner)],
                    Some(vec![Cursor { anchor: ns, head: ns + ilen }]),
                );
                return;
            }
            let wrapped = format!("{}{}{}", prefix, sel, suffix);
            let inner_s = s + prefix.len();
            let inner_e = inner_s + sel.len();
            self.apply_edits(
                vec![(s, e, wrapped)],
                Some(vec![Cursor { anchor: inner_s, head: inner_e }]),
            );
        } else {
            let wrapped = format!("{}{}{}", prefix, placeholder, suffix);
            let inner_s = s + prefix.len();
            let inner_e = inner_s + placeholder.len();
            self.apply_edits(
                vec![(s, e, wrapped)],
                Some(vec![Cursor { anchor: inner_s, head: inner_e }]),
            );
        }
    }

    pub fn insert_md_snippet(&mut self, snippet: &str, caret_off: usize, sel_len: usize, ensure_new_line: bool) {
        let (s, e) = self.primary().range();
        let prefix_nl = if ensure_new_line {
            let line = self.line_of_byte(s);
            let (ls, _) = self.line_bounds(line);
            if s > ls && !self.text[ls..s].trim().is_empty() {
                "\n"
            } else {
                ""
            }
        } else {
            ""
        };
        let full = format!("{}{}", prefix_nl, snippet);
        let base = s + prefix_nl.len();
        let anchor = (base + caret_off).min(s + full.len());
        let head = (anchor + sel_len).min(s + full.len());
        self.apply_edits(vec![(s, e, full)], Some(vec![Cursor { anchor, head }]));
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
        self.lang = detect_lang(path.as_deref());
        self.text = text;
        self.rebuild_line_index();
        self.wrap_patches.clear();
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
        floor_char_boundary(&self.text, pos)
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
            c.anchor = floor_char_boundary(&self.text, c.anchor);
            c.head = floor_char_boundary(&self.text, c.head);
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
        let added_count = new_lines.len();
        for s in &mut self.line_starts[end_line + 1..] {
            *s = (*s as isize + delta) as usize;
        }
        self.line_starts.splice((start_line + 1)..=end_line, new_lines);

        let old_span = end_line - start_line + 1;
        let new_span = added_count + 1;
        self.line_states.splice((start_line + 1)..=end_line, vec![LineState::Normal; added_count]);
        self.line_levels.splice((start_line + 1)..=end_line, vec![0; added_count]);
        self.line_folded.splice((start_line + 1)..=end_line, vec![false; added_count]);
        self.line_hidden.splice((start_line + 1)..=end_line, vec![false; added_count]);

        self.large_mode = self.text.len() > LARGE_FILE_BYTES;
        self.update_line_states_incremental(start_line, start_line + added_count);
        if self.line_folded.iter().any(|&f| f) {
            self.recompute_hidden_lines();
        }
        self.wrap_patches.push((start_line, old_span, new_span));
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
            Lang::Markdown => "> ",
            Lang::Plain | Lang::Json | Lang::XmlHtml | Lang::Cpp | Lang::JsTs => "// ",
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

fn expand_regex_template(template: &str, caps: &Captures<'_>) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let b = template.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() {
            let next = b[i + 1];
            if next == b'$' {
                out.push('$');
                i += 2;
                continue;
            }
            if next == b'&' {
                if let Some(m) = caps.get(0) {
                    out.push_str(m.as_str());
                }
                i += 2;
                continue;
            }
            if next == b'{' {
                if let Some(close_rel) = template[i + 2..].find('}') {
                    let inner = &template[i + 2..i + 2 + close_rel];
                    if let Ok(idx) = inner.parse::<usize>() {
                        if let Some(m) = caps.get(idx) {
                            out.push_str(m.as_str());
                        }
                        i = i + 2 + close_rel + 1;
                        continue;
                    }
                }
            }
            if next.is_ascii_digit() {
                let mut end = i + 2;
                if end < b.len() && b[end].is_ascii_digit() {
                    end += 1;
                }
                if let Ok(idx) = template[i + 1..end].parse::<usize>() {
                    if let Some(m) = caps.get(idx) {
                        out.push_str(m.as_str());
                    }
                    i = end;
                    continue;
                }
            }
        } else if b[i] == b'\\' && i + 1 < b.len() {
            let next = b[i + 1];
            match next {
                b'n' => {
                    out.push('\n');
                    i += 2;
                    continue;
                }
                b'r' => {
                    if i + 3 < b.len() && b[i + 2] == b'\\' && b[i + 3] == b'n' {
                        out.push('\n');
                        i += 4;
                    } else {
                        out.push('\n');
                        i += 2;
                    }
                    continue;
                }
                b't' => {
                    out.push('\t');
                    i += 2;
                    continue;
                }
                b'\\' => {
                    out.push('\\');
                    i += 2;
                    continue;
                }
                b'0'..=b'9' => {
                    let idx = (next - b'0') as usize;
                    if let Some(m) = caps.get(idx) {
                        out.push_str(m.as_str());
                    }
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        let ch = template[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    let (norm, _) = normalize_newlines_fast(&out);
    norm
}

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

    fn build_regex(&self) -> Result<Regex, String> {
        let mut pattern = if self.use_regex {
            self.query.clone()
        } else {
            regex_lite::escape(&self.query)
        };
        if self.whole_word {
            pattern = format!(r"\b(?:{})\b", pattern);
        }
        RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|e| e.to_string())
    }

    pub fn recompute(&mut self, text: &str, caret: usize) {
        self.matches.clear();
        self.current_idx = None;
        self.last_error = None;
        if self.query.is_empty() {
            return;
        }
        let re = match self.build_regex() {
            Ok(r) => r,
            Err(e) => {
                self.last_error = Some(e);
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

    pub fn expand_single_replace(&self, doc: &Document, ms: usize, me: usize) -> String {
        if !self.use_regex {
            let (norm, _) = normalize_newlines_fast(&self.replace_with);
            return norm;
        }
        let Ok(re) = self.build_regex() else {
            return self.replace_with.clone();
        };
        let l1 = doc.line_of_byte(ms);
        let l2 = doc.line_of_byte(me);
        let (ctx_s, _) = doc.line_bounds(l1);
        let (_, ctx_e) = doc.line_bounds(l2);
        let slice = &doc.text[ctx_s..ctx_e];
        for caps in re.captures_iter(slice) {
            if let Some(m) = caps.get(0) {
                if ctx_s + m.start() == ms && ctx_s + m.end() == me {
                    return expand_regex_template(&self.replace_with, &caps);
                }
            }
        }
        if let Some(caps) = re.captures(&doc.text[ms..me]) {
            return expand_regex_template(&self.replace_with, &caps);
        }
        self.replace_with.clone()
    }

    pub fn build_replace_all_edits(&mut self, text: &str) -> Vec<(usize, usize, String)> {
        self.last_error = None;
        if self.query.is_empty() {
            return Vec::new();
        }
        let re = match self.build_regex() {
            Ok(r) => r,
            Err(e) => {
                self.last_error = Some(e);
                return Vec::new();
            }
        };
        let mut edits = Vec::new();
        if !self.use_regex {
            let (lit, _) = normalize_newlines_fast(&self.replace_with);
            for m in re.find_iter(text) {
                if m.start() < m.end() {
                    edits.push((m.start(), m.end(), lit.clone()));
                }
            }
        } else {
            for caps in re.captures_iter(text) {
                if let Some(m) = caps.get(0) {
                    if m.start() < m.end() {
                        edits.push((m.start(), m.end(), expand_regex_template(&self.replace_with, &caps)));
                    }
                }
            }
        }
        edits
    }

    pub fn step_from_caret(&mut self, forward: bool, caret_sel: (usize, usize)) -> Option<(usize, usize)> {
        if self.matches.is_empty() {
            return None;
        }
        let len = self.matches.len();
        let next = if let Some(cur) = self.current_idx {
            if self.matches[cur] == caret_sel {
                if forward { (cur + 1) % len } else { (cur + len - 1) % len }
            } else if forward {
                self.matches.partition_point(|&(s, _)| s < caret_sel.1) % len
            } else {
                let p = self.matches.partition_point(|&(s, _)| s < caret_sel.0);
                if p == 0 { len - 1 } else { p - 1 }
            }
        } else if forward {
            self.matches.partition_point(|&(s, _)| s < caret_sel.1) % len
        } else {
            let p = self.matches.partition_point(|&(s, _)| s < caret_sel.0);
            if p == 0 { len - 1 } else { p - 1 }
        };
        self.current_idx = Some(next);
        Some(self.matches[next])
    }
}

// ============================================================================
// 6. Markdown / GFM 与多语言可见行语法高亮引擎
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
pub enum LineState {
    Normal,
    FencedCode {
        fence_ch: u8,
        fence_len: u8,
        inner_lang: Lang,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    Heading1,
    Heading2,
    Heading3,
    StringLit,
    Comment,
    Number,
    Accent,
    Bold,
    Italic,
    Strike,
    InlineCode,
    AlertTag,
    TaskDone,
    TaskTodo,
}

#[derive(Clone, Copy, Default)]
pub struct LineAttr {
    pub is_code_block: bool,
    pub is_quote: bool,
    pub is_hr: bool,
    pub is_table: bool,
}

pub fn detect_lang(path: Option<&Path>) -> Lang {
    let Some(p) = path else {
        return Lang::Markdown;
    };
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "json" | "jsonc" => Lang::Json,
        "xml" | "html" | "htm" | "svg" => Lang::XmlHtml,
        "py" | "pyw" => Lang::Python,
        "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" | "rs" | "go" | "java" => Lang::Cpp,
        "js" | "jsx" | "ts" | "tsx" | "mjs" => Lang::JsTs,
        "sh" | "bash" | "bat" | "cmd" | "ps1" => Lang::Shell,
        "ini" | "toml" | "cfg" | "conf" | "yaml" | "yml" => Lang::Ini,
        _ => Lang::Markdown,
    }
}

fn lang_from_fence_tag(tag: &str) -> Lang {
    let t = tag.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    match t.as_str() {
        "rs" | "rust" | "c" | "cpp" | "c++" | "h" | "hpp" | "go" | "java" | "cs" => Lang::Cpp,
        "py" | "python" => Lang::Python,
        "js" | "javascript" | "ts" | "typescript" | "jsx" | "tsx" => Lang::JsTs,
        "json" | "jsonc" => Lang::Json,
        "html" | "xml" | "svg" => Lang::XmlHtml,
        "sh" | "bash" | "zsh" | "shell" | "bat" | "cmd" | "ps1" | "powershell" => Lang::Shell,
        "ini" | "toml" | "yaml" | "yml" => Lang::Ini,
        _ => Lang::Plain,
    }
}

fn parse_fence_open(line: &str) -> Option<(u8, u8, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let ch = bytes[i];
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let start = i;
    while i < bytes.len() && bytes[i] == ch {
        i += 1;
    }
    let count = i - start;
    if count < 3 {
        return None;
    }
    let info = line[i..].trim();
    if ch == b'`' && info.contains('`') {
        return None;
    }
    Some((ch, count.min(255) as u8, info))
}

fn is_fence_close(line: &str, fence_ch: u8, fence_len: u8) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && bytes[i] == fence_ch {
        i += 1;
    }
    if i - start < fence_len as usize {
        return false;
    }
    line[i..].trim().is_empty()
}

pub fn advance_md_line_state(state: LineState, line: &str) -> LineState {
    match state {
        LineState::Normal => {
            if let Some((ch, len, info)) = parse_fence_open(line) {
                LineState::FencedCode {
                    fence_ch: ch,
                    fence_len: len,
                    inner_lang: lang_from_fence_tag(info),
                }
            } else {
                LineState::Normal
            }
        }
        LineState::FencedCode { fence_ch, fence_len, .. } => {
            if is_fence_close(line, fence_ch, fence_len) {
                LineState::Normal
            } else {
                state
            }
        }
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
                | "false" | "nullptr" | "NULL" | "include" | "define" | "func" | "package"
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

#[inline]
fn next_char_step(line: &str, i: usize, limit: usize) -> usize {
    if i >= limit {
        return limit;
    }
    let ch_len = line[i..limit].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    (i + ch_len).min(limit)
}

fn highlight_code_line(lang: Lang, line: &str, out: &mut Vec<(usize, usize, TokenKind)>) {
    if lang == Lang::Plain || line.is_empty() {
        return;
    }
    let bytes = line.as_bytes();
    let len = bytes.len();

    if lang == Lang::Ini {
        let ws = len - line.trim_start().len();
        if bytes.get(ws) == Some(&b';') || bytes.get(ws) == Some(&b'#') {
            out.push((ws, len, TokenKind::Comment));
            return;
        }
        if bytes.get(ws) == Some(&b'[') {
            out.push((ws, len, TokenKind::Keyword));
            return;
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
        if b == b'"' || b == b'\'' {
            let quote = b;
            let start = i;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' && i + 1 < len {
                    i = next_char_step(line, i + 1, len);
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i = next_char_step(line, i, len);
            }
            out.push((start, i, TokenKind::StringLit));
            continue;
        }
        if lang == Lang::XmlHtml && b == b'<' {
            let start = i;
            i += 1;
            while i < len && bytes[i] != b'>' && bytes[i] != b' ' {
                i = next_char_step(line, i, len);
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
        i = next_char_step(line, i, len);
    }
}

fn is_horizontal_rule(line: &str) -> bool {
    let t = line.trim();
    if t.len() < 3 {
        return false;
    }
    let ch = t.as_bytes()[0];
    if ch != b'-' && ch != b'*' && ch != b'_' {
        return false;
    }
    let count = t.bytes().filter(|&b| b == ch).count();
    let valid = t.bytes().all(|b| b == ch || b == b' ' || b == b'\t');
    valid && count >= 3
}

fn is_gfm_table_delimiter(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('-') || !t.contains('|') {
        return false;
    }
    t.bytes().all(|b| matches!(b, b'|' | b'-' | b':' | b' ' | b'\t'))
}

fn scan_inline_md(
    line: &str,
    from: usize,
    to: usize,
    base_kind: Option<TokenKind>,
    out: &mut Vec<(usize, usize, TokenKind)>,
) {
    let bytes = line.as_bytes();
    let mut i = from;
    let mut plain_start = from;

    let flush_plain = |ps: usize, pe: usize, out_vec: &mut Vec<(usize, usize, TokenKind)>| {
        if ps < pe {
            if let Some(bk) = base_kind {
                out_vec.push((ps, pe, bk));
            }
        }
    };

    while i < to {
        let b = bytes[i];
        let look_end = floor_char_boundary(line, (i + INLINE_LOOKAHEAD).min(to));

        if b == b'\\' && i + 1 < to {
            i = next_char_step(line, i + 1, to);
            continue;
        }

        if b == b'`' {
            let mut ticks = 0usize;
            while i + ticks < to && bytes[i + ticks] == b'`' {
                ticks += 1;
            }
            let mut j = i + ticks;
            let mut found = None;
            while j < look_end {
                if bytes[j] == b'`' {
                    let mut c = 0usize;
                    while j + c < look_end && bytes[j + c] == b'`' {
                        c += 1;
                    }
                    if c == ticks {
                        found = Some(j + c);
                        break;
                    }
                    j += c;
                } else {
                    j = next_char_step(line, j, look_end);
                }
            }
            if let Some(end) = found {
                flush_plain(plain_start, i, out);
                out.push((i, end, TokenKind::InlineCode));
                i = end;
                plain_start = i;
                continue;
            }
            i += ticks;
            continue;
        }

        if base_kind.is_none() && b == b'|' {
            flush_plain(plain_start, i, out);
            out.push((i, i + 1, TokenKind::Accent));
            i += 1;
            plain_start = i;
            continue;
        }

        if (b == b'!' && i + 1 < to && bytes[i + 1] == b'[') || b == b'[' {
            let lb = if b == b'!' { i + 1 } else { i };
            if lb + 1 < look_end {
                if let Some(rb_rel) = line[lb + 1..look_end].find(']') {
                    let rb = lb + 1 + rb_rel;
                    if rb + 1 < look_end && bytes[rb + 1] == b'(' {
                        if let Some(rp_rel) = line[rb + 2..look_end].find(')') {
                            let rp = rb + 2 + rp_rel + 1;
                            flush_plain(plain_start, i, out);
                            out.push((i, rb + 1, TokenKind::Accent));
                            out.push((rb + 1, rp, TokenKind::StringLit));
                            i = rp;
                            plain_start = i;
                            continue;
                        }
                    }
                }
            }
        }

        if b == b'<' && i + 1 < look_end {
            if let Some(gt_rel) = line[i + 1..look_end].find('>') {
                let inner = &line[i + 1..i + 1 + gt_rel];
                if inner.starts_with("http://") || inner.starts_with("https://") || inner.contains('@') {
                    let end = i + 1 + gt_rel + 1;
                    flush_plain(plain_start, i, out);
                    out.push((i, end, TokenKind::StringLit));
                    i = end;
                    plain_start = i;
                    continue;
                }
            }
        }

        if b == b'~' && i + 1 < to && bytes[i + 1] == b'~' && i + 2 < look_end {
            if let Some(end_rel) = line[i + 2..look_end].find("~~") {
                if end_rel > 0 {
                    let end = i + 2 + end_rel + 2;
                    flush_plain(plain_start, i, out);
                    out.push((i, end, TokenKind::Strike));
                    i = end;
                    plain_start = i;
                    continue;
                }
            }
        }

        if (b == b'*' || b == b'_') && i + 1 < to && bytes[i + 1] == b && i + 2 < look_end {
            let pat = if b == b'*' { "**" } else { "__" };
            let word_ok = b == b'*' || i == from || !is_ident_char(bytes[i - 1]);
            if word_ok {
                if let Some(end_rel) = line[i + 2..look_end].find(pat) {
                    if end_rel > 0 {
                        let end = i + 2 + end_rel + 2;
                        flush_plain(plain_start, i, out);
                        out.push((i, end, TokenKind::Bold));
                        i = end;
                        plain_start = i;
                        continue;
                    }
                }
            }
        }

        if b == b'*' || b == b'_' {
            let word_ok = b == b'*' || i == from || !is_ident_char(bytes[i - 1]);
            if word_ok && i + 1 < look_end && bytes[i + 1] != b' ' && bytes[i + 1] != b {
                let mut j = i + 1;
                let mut found = None;
                while j < look_end {
                    if bytes[j] == b'\\' && j + 1 < look_end {
                        j = next_char_step(line, j + 1, look_end);
                        continue;
                    }
                    if bytes[j] == b && bytes[j - 1] != b' ' {
                        if b == b'*' || j + 1 >= to || !is_ident_char(bytes[j + 1]) {
                            found = Some(j + 1);
                            break;
                        }
                    }
                    j = next_char_step(line, j, look_end);
                }
                if let Some(end) = found {
                    flush_plain(plain_start, i, out);
                    out.push((i, end, TokenKind::Italic));
                    i = end;
                    plain_start = i;
                    continue;
                }
            }
        }

        i = next_char_step(line, i, to);
    }
    flush_plain(plain_start, to, out);
}

pub fn highlight_line_with_state(
    lang: Lang,
    line: &str,
    state: LineState,
) -> (Vec<(usize, usize, TokenKind)>, LineAttr) {
    let mut out = Vec::new();
    let mut attr = LineAttr::default();
    if line.is_empty() || lang == Lang::Plain {
        return (out, attr);
    }
    if lang != Lang::Markdown {
        highlight_code_line(lang, line, &mut out);
        return (out, attr);
    }

    let len = line.len();
    let bytes = line.as_bytes();

    if let LineState::FencedCode { fence_ch, fence_len, inner_lang } = state {
        attr.is_code_block = true;
        if is_fence_close(line, fence_ch, fence_len) {
            out.push((0, len, TokenKind::Accent));
        } else {
            highlight_code_line(inner_lang, line, &mut out);
        }
        return (out, attr);
    }

    if parse_fence_open(line).is_some() {
        attr.is_code_block = true;
        let ws = len - line.trim_start().len();
        let mut fence_end = ws;
        while fence_end < len && (bytes[fence_end] == b'`' || bytes[fence_end] == b'~') {
            fence_end += 1;
        }
        out.push((ws, fence_end, TokenKind::Accent));
        if fence_end < len {
            out.push((fence_end, len, TokenKind::Keyword));
        }
        return (out, attr);
    }

    let ws = len - line.trim_start().len();
    let rest = &line[ws..];

    if (ws >= 4 || bytes.first() == Some(&b'\t')) && !rest.is_empty() {
        attr.is_code_block = true;
        out.push((0, len, TokenKind::InlineCode));
        return (out, attr);
    }

    if ws <= 3 && is_horizontal_rule(rest) {
        attr.is_hr = true;
        out.push((ws, len, TokenKind::Comment));
        return (out, attr);
    }

    if ws <= 3 && rest.starts_with('#') {
        let mut hashes = 0usize;
        while hashes < rest.len() && rest.as_bytes()[hashes] == b'#' {
            hashes += 1;
        }
        if (1..=6).contains(&hashes) && (hashes == rest.len() || rest.as_bytes()[hashes] == b' ' || rest.as_bytes()[hashes] == b'\t') {
            let h_kind = match hashes {
                1 => TokenKind::Heading1,
                2 => TokenKind::Heading2,
                _ => TokenKind::Heading3,
            };
            out.push((ws, ws + hashes, h_kind));
            if ws + hashes < len {
                out.push((ws + hashes, len, h_kind));
            }
            return (out, attr);
        }
    }

    if ws <= 3 && rest.starts_with('>') {
        attr.is_quote = true;
        let mut q_end = ws;
        while q_end < len && (bytes[q_end] == b'>' || bytes[q_end] == b' ') {
            q_end += 1;
        }
        out.push((ws, q_end, TokenKind::Accent));
        let q_rest = &line[q_end..];
        let upper_prefix: String = q_rest.chars().take(14).collect::<String>().to_ascii_uppercase();
        let alert_match = ["[!NOTE]", "[!TIP]", "[!IMPORTANT]", "[!WARNING]", "[!CAUTION]"]
            .iter()
            .find(|&&tag| upper_prefix.starts_with(tag))
            .copied();
        if let Some(tag) = alert_match {
            let tag_end = q_end + tag.len();
            out.push((q_end, tag_end, TokenKind::AlertTag));
            scan_inline_md(line, tag_end, len, Some(TokenKind::Comment), &mut out);
        } else {
            scan_inline_md(line, q_end, len, Some(TokenKind::Comment), &mut out);
        }
        return (out, attr);
    }

    if is_gfm_table_delimiter(line) {
        attr.is_table = true;
        out.push((ws, len, TokenKind::Accent));
        return (out, attr);
    }
    if rest.starts_with('|') {
        attr.is_table = true;
    }

    let mut content_start = ws;
    if ws <= 3 {
        let rb = rest.as_bytes();
        if !rb.is_empty() && matches!(rb[0], b'-' | b'*' | b'+') && rb.get(1).copied().is_some_and(|c| c == b' ' || c == b'\t') {
            out.push((ws, ws + 1, TokenKind::Number));
            content_start = ws + 2;
            let after = &line[content_start..];
            if after.starts_with("[ ]") && (after.len() == 3 || after.as_bytes()[3] == b' ') {
                out.push((content_start, content_start + 3, TokenKind::TaskTodo));
                content_start += 3;
            } else if (after.starts_with("[x]") || after.starts_with("[X]"))
                && (after.len() == 3 || after.as_bytes()[3] == b' ')
            {
                out.push((content_start, content_start + 3, TokenKind::TaskDone));
                content_start += 3;
            }
        } else {
            let mut d = 0usize;
            while d < rb.len() && rb[d].is_ascii_digit() && d < 9 {
                d += 1;
            }
            if d > 0 && d + 1 < rb.len() && (rb[d] == b'.' || rb[d] == b')') && (rb[d + 1] == b' ' || rb[d + 1] == b'\t') {
                out.push((ws, ws + d + 1, TokenKind::Number));
                content_start = ws + d + 2;
            }
        }
    }

    scan_inline_md(line, content_start, len, None, &mut out);
    (out, attr)
}

// ============================================================================
// 7. Markdown 大纲解析器
// ============================================================================

#[derive(Clone)]
pub struct OutlineNode {
    pub level: u8,
    pub title: String,
    pub line: usize,
    pub byte_pos: usize,
    pub has_children: bool,
    pub collapsed: bool,
}

pub struct OutlineState {
    pub nodes: Vec<OutlineNode>,
    pub selected_idx: Option<usize>,
    pub scroll_row: usize,
}

impl OutlineState {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            selected_idx: None,
            scroll_row: 0,
        }
    }

    pub fn refresh_from_doc(&mut self, doc: &Document) {
        let prev_collapsed: Vec<(u8, String)> = self
            .nodes
            .iter()
            .filter(|n| n.collapsed)
            .map(|n| (n.level, n.title.clone()))
            .collect();

        let mut parsed = Vec::new();
        let line_cnt = doc.line_count();
        let mut in_fence: Option<(u8, u8)> = None;

        for l in 0..line_cnt {
            let line = doc.line_str(l);
            if let Some((fc, fl)) = in_fence {
                if is_fence_close(line, fc, fl) {
                    in_fence = None;
                }
                continue;
            }
            if let Some((fc, fl, _)) = parse_fence_open(line) {
                in_fence = Some((fc, fl));
                continue;
            }

            let trimmed = line.trim_start();
            let ws = line.len() - trimmed.len();
            if ws <= 3 && trimmed.starts_with('#') {
                let mut hashes = 0usize;
                while hashes < trimmed.len() && trimmed.as_bytes()[hashes] == b'#' {
                    hashes += 1;
                }
                if (1..=6).contains(&hashes)
                    && (hashes == trimmed.len() || trimmed.as_bytes()[hashes] == b' ' || trimmed.as_bytes()[hashes] == b'\t')
                {
                    let raw = trimmed[hashes..].trim();
                    let title = raw.trim_end_matches('#').trim_end().to_string();
                    if !title.is_empty() {
                        parsed.push(OutlineNode {
                            level: hashes as u8,
                            title,
                            line: l,
                            byte_pos: doc.line_starts[l],
                            has_children: false,
                            collapsed: false,
                        });
                    }
                    continue;
                }
            }

            if l > 0 && ws <= 3 {
                let t = trimmed.trim_end();
                if t.len() >= 3 && (t.bytes().all(|b| b == b'=') || t.bytes().all(|b| b == b'-')) {
                    let prev_line = doc.line_str(l - 1).trim();
                    if !prev_line.is_empty()
                        && !prev_line.starts_with('#')
                        && !prev_line.starts_with('>')
                        && !prev_line.starts_with("```")
                        && !prev_line.starts_with('|')
                        && parsed.last().map(|n: &OutlineNode| n.line != l - 1).unwrap_or(true)
                    {
                        let lvl = if t.as_bytes()[0] == b'=' { 1 } else { 2 };
                        parsed.push(OutlineNode {
                            level: lvl,
                            title: prev_line.to_string(),
                            line: l - 1,
                            byte_pos: doc.line_starts[l - 1],
                            has_children: false,
                            collapsed: false,
                        });
                    }
                }
            }
        }

        for i in 0..parsed.len() {
            if i + 1 < parsed.len() && parsed[i + 1].level > parsed[i].level {
                parsed[i].has_children = true;
            }
            if prev_collapsed.iter().any(|(lv, t)| *lv == parsed[i].level && t == &parsed[i].title) && parsed[i].has_children {
                parsed[i].collapsed = true;
            }
        }

        self.nodes = parsed;
        self.selected_idx = None;
        self.scroll_row = 0;
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let mut vis = Vec::with_capacity(self.nodes.len());
        let mut hide_above_level: Option<u8> = None;
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(lvl) = hide_above_level {
                if node.level > lvl {
                    continue;
                } else {
                    hide_above_level = None;
                }
            }
            vis.push(i);
            if node.collapsed && node.has_children {
                hide_above_level = Some(node.level);
            }
        }
        vis
    }

    pub fn toggle_collapse(&mut self, idx: usize) {
        if let Some(n) = self.nodes.get_mut(idx) {
            if n.has_children {
                n.collapsed = !n.collapsed;
            }
        }
    }

    pub fn set_all_collapsed(&mut self, collapsed: bool) {
        for n in &mut self.nodes {
            if n.has_children {
                n.collapsed = collapsed;
            }
        }
        self.scroll_row = 0;
    }
}
