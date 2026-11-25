use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::{BST_CHECKED, EM_SETSEL};
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::engine::*;

const STATUS_BAR_HEIGHT: i32 = 24;
const FIND_PANEL_W: i32 = 430;
const FIND_PANEL_H: i32 = 108;
const CF_UNICODETEXT_ID: u32 = 13;
const SS_CENTER: u32 = 1;

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

// ============================================================================
// 1. 主题配色与辅助函数
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

#[derive(Clone, Copy)]
pub struct VisualSeg {
    pub line: usize,
    pub b_start: usize,
    pub b_end: usize,
}

pub struct AppState {
    pub hwnd: HWND,
    pub hfont_editor: HFONT,
    pub hfont_ui: HFONT,
    pub doc: Document,
    pub cfg: AppConfig,
    pub search: SearchState,
    pub lang: Lang,

    pub client_w: i32,
    pub client_h: i32,
    pub line_h: i32,
    pub avg_char_w: i32,
    pub gutter_w: i32,

    pub scroll_y: usize,
    pub scroll_x: i32,
    pub visual_lines: Vec<VisualSeg>,

    pub dragging: bool,
    pub rect_selecting: bool,
    pub rect_anchor: (usize, usize),

    pub find_panel: HWND,
    pub find_edit: HWND,
    pub replace_edit: HWND,
    pub chk_case: HWND,
    pub chk_word: HWND,
    pub chk_regex: HWND,
    pub lbl_count: HWND,
    pub orig_find_edit_proc: isize,
    pub orig_rep_edit_proc: isize,

    pub goto_hwnd: HWND,
    pub goto_edit: HWND,
}

impl AppState {
    fn effective_font_px(&self) -> i32 {
        ((self.cfg.font_size * self.cfg.zoom_pct) / 100).clamp(8, 72)
    }

    pub fn recreate_fonts(&mut self) {
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
                (FIXED_PITCH as u32) | (FF_MODERN as u32),
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

        let mut bounds: Vec<usize> = seg_str.char_indices().map(|(i, _)| i).collect();
        bounds.push(seg_str.len());

        let mut best_idx;
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
            SendMessageW(self.find_edit, EM_SETSEL as u32, 0, -1);
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

    pub fn open_file_path(&mut self, path: &std::path::Path, forced_enc: Option<DocEncoding>) {
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

// ============================================================================
// 2. 双缓冲 GDI 虚拟滚动渲染器（仅重绘可见行 + 状态栏）
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

        let bg_br = CreateSolidBrush(theme.bg);
        let full_rc = RECT { left: 0, top: 0, right: w, bottom: h - STATUS_BAR_HEIGHT };
        FillRect(mem_dc, &full_rc, bg_br);
        DeleteObject(bg_br as _);

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

            if seg.line == cur_line {
                let row_rc = RECT {
                    left: state.gutter_w,
                    top: y,
                    right: w,
                    bottom: y + state.line_h,
                };
                FillRect(mem_dc, &row_rc, cur_line_br);
            }

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

            SelectClipRgn(mem_dc, clip_rgn);

            let line_abs_start = state.doc.line_starts[seg.line];
            let seg_abs_start = line_abs_start + seg.b_start;
            let seg_abs_end = line_abs_start + seg.b_end;
            let full_line = state.doc.line_str(seg.line);
            let seg_str = &full_line[seg.b_start..seg.b_end];

            let draw_slice = if seg_str.len() > 16_384 {
                let mut e = 16_384;
                while !seg_str.is_char_boundary(e) {
                    e -= 1;
                }
                &seg_str[..e]
            } else {
                seg_str
            };

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

            let tokens = if draw_slice.len() <= 8192 {
                highlight_line(active_lang, full_line)
            } else {
                Vec::new()
            };

            let mut cur_x = text_left;
            let mut pos = 0usize;
            let draw_chunk = |chunk: &str, color: u32, x_acc: &mut i32| {
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

        // 底部紧凑状态栏
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
// 3. 剪贴板、系统原生打开/保存/字体对话框
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
// 4. 菜单构建、悬浮查找面板、转到行弹窗与快捷键分发
// ============================================================================

pub fn build_main_menu() -> HMENU {
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
            SendMessageW(target, EM_SETSEL as u32, 0, -1);
            return 0;
        }
    }
    let prev: WNDPROC = std::mem::transmute(orig);
    CallWindowProcW(prev, hwnd, msg, wp, lp)
}

unsafe extern "system" fn find_panel_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let main_hwnd = GetParent(hwnd);
    let state_ptr = GetWindowLongPtrW(main_hwnd, GWLP_USERDATA) as *mut AppState;
    if msg == WM_COMMAND && !state_ptr.is_null() {
        let st = &mut *state_ptr;
        let id = loword(wp) as i32;
        let code = hiword(wp) as u32;
        match id {
            IDC_FIND_EDIT if code == EN_CHANGE as u32 => {
                st.search.query = get_win_text(st.find_edit);
                st.refresh_search_ui();
                return 0;
            }
            IDC_REPLACE_EDIT if code == EN_CHANGE as u32 => {
                st.search.replace_with = get_win_text(st.replace_edit);
                return 0;
            }
            IDC_CHK_CASE | IDC_CHK_WORD | IDC_CHK_REGEX => {
                st.search.case_sensitive =
                    SendMessageW(st.chk_case, BM_GETCHECK as u32, 0, 0) == BST_CHECKED as isize;
                st.search.whole_word =
                    SendMessageW(st.chk_word, BM_GETCHECK as u32, 0, 0) == BST_CHECKED as isize;
                st.search.use_regex =
                    SendMessageW(st.chk_regex, BM_GETCHECK as u32, 0, 0) == BST_CHECKED as isize;
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

pub fn create_find_panel(state: &mut AppState, hinst: HINSTANCE) {
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
            let ch = CreateWindowExW(
                0,
                wc.as_ptr(),
                wt.as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                x,
                y,
                w,
                h,
                panel,
                id as isize as HMENU,
                hinst,
                null(),
            );
            SendMessageW(ch, WM_SETFONT as u32, state.hfont_ui as usize, 1);
            ch
        };

        state.find_edit = mk("EDIT", "", WS_BORDER | ES_AUTOHSCROLL as u32, 8, 8, 200, 24, IDC_FIND_EDIT);
        state.lbl_count = mk("STATIC", "0 / 0", SS_CENTER, 214, 12, 74, 20, IDC_LBL_COUNT);
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

unsafe extern "system" fn goto_edit_subclass(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let orig = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if msg == WM_KEYDOWN {
        let dlg = GetParent(hwnd);
        if wp == VK_RETURN as usize {
            SendMessageW(dlg, WM_COMMAND, IDC_GOTO_OK as usize, 0);
            return 0;
        }
        if wp == VK_ESCAPE as usize {
            SendMessageW(dlg, WM_COMMAND, IDC_GOTO_CANCEL as usize, 0);
            return 0;
        }
    }
    let prev: WNDPROC = std::mem::transmute(orig);
    CallWindowProcW(prev, hwnd, msg, wp, lp)
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
            125,
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
        SendMessageW(h1, WM_SETFONT as u32, state.hfont_ui as usize, 1);

        let ed_cls = to_wide("EDIT");
        let (cur_l, _) = state.doc.byte_to_line_col(state.doc.primary().head);
        let init_v = to_wide(&(cur_l + 1).to_string());
        let ed = CreateWindowExW(
            0,
            ed_cls.as_ptr(),
            init_v.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | ES_NUMBER as u32,
            12,
            30,
            220,
            24,
            dlg,
            IDC_GOTO_EDIT as isize as HMENU,
            hinst,
            null(),
        );
        SendMessageW(ed, WM_SETFONT as u32, state.hfont_ui as usize, 1);
        let orig_ed = SetWindowLongPtrW(ed, GWLP_WNDPROC, goto_edit_subclass as isize);
        SetWindowLongPtrW(ed, GWLP_USERDATA, orig_ed);
        state.goto_edit = ed;

        let bt_cls = to_wide("BUTTON");
        let ok_t = to_wide("确定");
        let ca_t = to_wide("取消");
        let b1 = CreateWindowExW(0, bt_cls.as_ptr(), ok_t.as_ptr(), WS_CHILD | WS_VISIBLE | BS_DEFPUSHBUTTON as u32, 82, 62, 70, 24, dlg, IDC_GOTO_OK as isize as HMENU, hinst, null());
        let b2 = CreateWindowExW(0, bt_cls.as_ptr(), ca_t.as_ptr(), WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32, 162, 62, 70, 24, dlg, IDC_GOTO_CANCEL as isize as HMENU, hinst, null());
        SendMessageW(b1, WM_SETFONT as u32, state.hfont_ui as usize, 1);
        SendMessageW(b2, WM_SETFONT as u32, state.hfont_ui as usize, 1);
        SetFocus(ed);
        SendMessageW(ed, EM_SETSEL as u32, 0, -1);
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
                    st_select_and_scroll(state, ms, me);
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

fn st_select_and_scroll(state: &mut AppState, ms: usize, me: usize) {
    state.doc.cursors = vec![Cursor { anchor: ms, head: me }];
    state.ensure_caret_visible();
    state.invalidate();
}

fn handle_keydown(state: &mut AppState, vk: u16) -> bool {
    let ctrl = key_down(VK_CONTROL);
    let shift = key_down(VK_SHIFT);
    let alt = key_down(VK_MENU);

    if ctrl && !alt {
        match vk as i32 {
            0x4E => { handle_command(state, IDM_FILE_NEW); return true; }
            0x4F => { handle_command(state, IDM_FILE_OPEN); return true; }
            0x53 => { handle_command(state, if shift { IDM_FILE_SAVEAS } else { IDM_FILE_SAVE }); return true; }
            0x5A => { handle_command(state, IDM_EDIT_UNDO); return true; }
            0x59 => { handle_command(state, IDM_EDIT_REDO); return true; }
            0x58 => { handle_command(state, IDM_EDIT_CUT); return true; }
            0x43 => { handle_command(state, IDM_EDIT_COPY); return true; }
            0x56 => { handle_command(state, IDM_EDIT_PASTE); return true; }
            0x41 => { handle_command(state, IDM_EDIT_SELECT_ALL); return true; }
            0x44 => { handle_command(state, IDM_EDIT_DUP_LINE); return true; }
            0x4B if shift => { handle_command(state, IDM_EDIT_DEL_LINE); return true; }
            0x55 => { handle_command(state, if shift { IDM_EDIT_UPPER } else { IDM_EDIT_LOWER }); return true; }
            0x46 => { handle_command(state, IDM_SEARCH_FIND); return true; }
            0x48 => { handle_command(state, IDM_SEARCH_REPLACE); return true; }
            0x47 => { handle_command(state, IDM_SEARCH_GOTO); return true; }
            0xBF => { handle_command(state, IDM_EDIT_COMMENT); return true; }
            0xBB => { handle_command(state, IDM_VIEW_ZOOM_IN); return true; }
            0xBD => { handle_command(state, IDM_VIEW_ZOOM_OUT); return true; }
            0x30 => { handle_command(state, IDM_VIEW_ZOOM_RESET); return true; }
            _ => {}
        }
    }
    if alt && !ctrl {
        match vk {
            VK_UP => { handle_command(state, IDM_EDIT_MOVE_UP); return true; }
            VK_DOWN => { handle_command(state, IDM_EDIT_MOVE_DOWN); return true; }
            0x5A => { handle_command(state, IDM_VIEW_WRAP); return true; }
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
// 5. Win32 主窗口过程
// ============================================================================

pub unsafe extern "system" fn main_wnd_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
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
            if loword(lp as usize) as u32 == HTCLIENT as u32 {
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
                } else if let Some(last) = st.doc.cursors.last_mut() {
                    last.head = pos;
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
            let code = loword(wp) as i32;
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
            let code = loword(wp) as i32;
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
