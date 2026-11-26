use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::{BST_CHECKED, EM_SETSEL, SetScrollInfo};
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::engine::*;
use crate::{
    IDM_EDIT_COMMENT, IDM_EDIT_COPY, IDM_EDIT_CUT, IDM_EDIT_DEL_LINE, IDM_EDIT_DUP_LINE,
    IDM_EDIT_LOWER, IDM_EDIT_MOVE_DOWN, IDM_EDIT_MOVE_UP, IDM_EDIT_PASTE, IDM_EDIT_REDO,
    IDM_EDIT_SELECT_ALL, IDM_EDIT_UNDO, IDM_EDIT_UPPER, IDM_EOL_CRLF, IDM_EOL_LF, IDM_FILE_EXIT,
    IDM_FILE_NEW, IDM_FILE_OPEN, IDM_FILE_RELOAD, IDM_FILE_SAVE, IDM_FILE_SAVEAS,
    IDM_SEARCH_FIND, IDM_SEARCH_GOTO, IDM_SEARCH_NEXT, IDM_SEARCH_PREV, IDM_SEARCH_REPLACE,
    IDM_VIEW_FONT, IDM_VIEW_LINENUM, IDM_VIEW_OUTLINE, IDM_VIEW_OUTLINE_REFRESH, IDM_VIEW_THEME,
    IDM_VIEW_TOPMOST, IDM_VIEW_WHITESPACE, IDM_VIEW_WRAP, IDM_VIEW_ZOOM_IN, IDM_VIEW_ZOOM_OUT,
    IDM_VIEW_ZOOM_RESET,
};

const STATUS_BAR_HEIGHT: i32 = 24;
const FIND_PANEL_W: i32 = 430;
const FIND_PANEL_H: i32 = 108;
const OUTLINE_HEADER_H: i32 = 30;
const OUTLINE_ROW_H: i32 = 24;
const CF_UNICODETEXT_ID: u32 = 13;
const SS_CENTER: u32 = 1;

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
// 1. 主题配色与 Tab 制表位展开辅助
// ============================================================================

#[derive(Clone, Copy)]
struct Theme {
    bg: u32,
    gutter_bg: u32,
    gutter_fg: u32,
    cur_line_bg: u32,
    code_block_bg: u32,
    inline_code_bg: u32,
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
    outline_bg: u32,
    outline_hdr_bg: u32,
    outline_sel_bg: u32,
    outline_btn_bg: u32,
    outline_fg: u32,
    outline_muted: u32,
    kw_fg: u32,
    h1_fg: u32,
    h2_fg: u32,
    str_fg: u32,
    cmt_fg: u32,
    num_fg: u32,
    acc_fg: u32,
    bold_fg: u32,
    italic_fg: u32,
    strike_fg: u32,
    code_fg: u32,
    alert_fg: u32,
    task_done_fg: u32,
    task_todo_fg: u32,
}

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

fn get_theme(dark: bool) -> Theme {
    if dark {
        Theme {
            bg: rgb(30, 30, 30),
            gutter_bg: rgb(35, 35, 37),
            gutter_fg: rgb(125, 125, 125),
            cur_line_bg: rgb(40, 43, 47),
            code_block_bg: rgb(37, 39, 44),
            inline_code_bg: rgb(52, 56, 64),
            sel_bg: rgb(38, 79, 120),
            match_bg: rgb(90, 70, 30),
            cur_match_bg: rgb(160, 110, 30),
            bracket_bg: rgb(65, 95, 65),
            text_fg: rgb(216, 216, 216),
            caret: rgb(240, 240, 240),
            ws_fg: rgb(85, 85, 85),
            status_bg: rgb(0, 122, 204),
            status_fg: rgb(255, 255, 255),
            status_border: rgb(50, 50, 54),
            outline_bg: rgb(36, 37, 40),
            outline_hdr_bg: rgb(45, 46, 50),
            outline_sel_bg: rgb(55, 76, 102),
            outline_btn_bg: rgb(60, 62, 68),
            outline_fg: rgb(220, 220, 220),
            outline_muted: rgb(145, 150, 160),
            kw_fg: rgb(86, 156, 214),
            h1_fg: rgb(97, 175, 239),
            h2_fg: rgb(128, 203, 196),
            str_fg: rgb(206, 145, 120),
            cmt_fg: rgb(128, 168, 112),
            num_fg: rgb(181, 206, 168),
            acc_fg: rgb(140, 200, 245),
            bold_fg: rgb(229, 192, 123),
            italic_fg: rgb(198, 170, 221),
            strike_fg: rgb(135, 135, 135),
            code_fg: rgb(224, 158, 116),
            alert_fg: rgb(255, 184, 108),
            task_done_fg: rgb(115, 201, 145),
            task_todo_fg: rgb(229, 192, 123),
        }
    } else {
        Theme {
            bg: rgb(255, 255, 255),
            gutter_bg: rgb(246, 246, 248),
            gutter_fg: rgb(130, 130, 130),
            cur_line_bg: rgb(242, 247, 255),
            code_block_bg: rgb(246, 248, 250),
            inline_code_bg: rgb(235, 238, 242),
            sel_bg: rgb(173, 214, 255),
            match_bg: rgb(255, 235, 150),
            cur_match_bg: rgb(255, 180, 70),
            bracket_bg: rgb(205, 238, 205),
            text_fg: rgb(32, 35, 40),
            caret: rgb(0, 0, 0),
            ws_fg: rgb(195, 195, 195),
            status_bg: rgb(240, 242, 245),
            status_fg: rgb(45, 48, 55),
            status_border: rgb(215, 218, 222),
            outline_bg: rgb(248, 249, 251),
            outline_hdr_bg: rgb(238, 240, 244),
            outline_sel_bg: rgb(214, 231, 250),
            outline_btn_bg: rgb(226, 229, 235),
            outline_fg: rgb(35, 38, 45),
            outline_muted: rgb(115, 120, 130),
            kw_fg: rgb(0, 92, 197),
            h1_fg: rgb(9, 105, 218),
            h2_fg: rgb(5, 118, 126),
            str_fg: rgb(163, 21, 21),
            cmt_fg: rgb(87, 122, 92),
            num_fg: rgb(9, 134, 88),
            acc_fg: rgb(4, 81, 165),
            bold_fg: rgb(149, 56, 0),
            italic_fg: rgb(130, 80, 223),
            strike_fg: rgb(145, 150, 158),
            code_fg: rgb(164, 53, 30),
            alert_fg: rgb(188, 76, 0),
            task_done_fg: rgb(26, 127, 55),
            task_todo_fg: rgb(154, 103, 0),
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

fn expand_tabs_col(s: &str, start_col: usize, show_ws: bool) -> (String, usize) {
    let mut out = String::with_capacity(s.len() + 8);
    let mut col = start_col;
    for ch in s.chars() {
        if ch == '\t' {
            let spaces = 4 - (col % 4);
            if show_ws {
                out.push('→');
                for _ in 1..spaces {
                    out.push(' ');
                }
            } else {
                for _ in 0..spaces {
                    out.push(' ');
                }
            }
            col += spaces;
        } else if ch == ' ' && show_ws {
            out.push('·');
            col += 1;
        } else {
            out.push(ch);
            col += if (ch as u32) > 0x7F { 2 } else { 1 };
        }
    }
    (out, col)
}

/// 展开 Tab 并建立 UTF-16 索引到原切片 UTF-8 字节偏移的映射
fn expand_tabs_with_u16_map(s: &str, half_snap: bool) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len() + 8);
    let mut map = Vec::with_capacity(s.len() + 4);
    let mut col = 0usize;
    for (b_idx, ch) in s.char_indices() {
        if ch == '\t' {
            let spaces = 4 - (col % 4);
            for k in 0..spaces {
                out.push(' ');
                if half_snap && k >= spaces / 2 {
                    map.push(b_idx + 1);
                } else {
                    map.push(b_idx);
                }
            }
            col += spaces;
        } else {
            let u16_len = ch.len_utf16();
            out.push(ch);
            for _ in 0..u16_len {
                map.push(b_idx);
            }
            col += if (ch as u32) > 0x7F { 2 } else { 1 };
        }
    }
    map.push(s.len());
    (out, map)
}

#[derive(Clone, Copy)]
pub struct VisualSeg {
    pub line: usize,
    pub b_start: usize,
    pub b_end: usize,
}

pub struct AppState {
    pub hwnd: HWND,
    pub hfont_regular: HFONT,
    pub hfont_bold: HFONT,
    pub hfont_italic: HFONT,
    pub hfont_strike: HFONT,
    pub hfont_ui: HFONT,
    pub hfont_ui_bold: HFONT,
    pub doc: Document,
    pub cfg: AppConfig,
    pub search: SearchState,
    pub outline: OutlineState,

    pub client_w: i32,
    pub client_h: i32,
    pub line_h: i32,
    pub avg_char_w: i32,
    pub gutter_w: i32,

    pub scroll_y: usize,
    pub scroll_x: i32,
    pub line_wraps: Vec<Vec<(usize, usize)>>,
    pub visual_prefix: Vec<usize>,

    pub dragging: bool,
    pub rect_selecting: bool,
    pub rect_anchor: (usize, usize),
    pub resizing_outline: bool,

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
            for f in [self.hfont_regular, self.hfont_bold, self.hfont_italic, self.hfont_strike] {
                if !f.is_null() {
                    DeleteObject(f as _);
                }
            }
            let px = self.effective_font_px();
            let wname = to_wide(&self.cfg.font_name);
            let mk_font = |weight: u32, italic: u32, strike: u32| -> HFONT {
                CreateFontW(
                    -px,
                    0,
                    0,
                    0,
                    weight as i32,
                    italic,
                    0,
                    strike,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    (FIXED_PITCH as u32) | (FF_MODERN as u32),
                    wname.as_ptr(),
                )
            };
            self.hfont_regular = mk_font(FW_NORMAL, 0, 0);
            self.hfont_bold = mk_font(FW_BOLD, 0, 0);
            self.hfont_italic = mk_font(FW_NORMAL, 1, 0);
            self.hfont_strike = mk_font(FW_NORMAL, 0, 1);

            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_regular as _);
            let mut tm: TEXTMETRICW = std::mem::zeroed();
            GetTextMetricsW(hdc, &mut tm);
            self.line_h = (tm.tmHeight + tm.tmExternalLeading + 6).max(16);
            self.avg_char_w = tm.tmAveCharWidth.max(6);
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
        }
        self.update_gutter_width();
        self.rebuild_all_wraps();
    }

    fn update_gutter_width(&mut self) {
        if !self.cfg.show_line_numbers {
            self.gutter_w = 14;
            return;
        }
        let digits = self.doc.line_count().max(1).to_string().len().max(3);
        self.gutter_w = (digits as i32 * self.avg_char_w) + 22;
    }

    fn outline_panel_width(&self) -> i32 {
        if self.cfg.outline_visible && self.client_w > 460 {
            self.cfg.outline_width.clamp(180, (self.client_w / 2).max(180))
        } else {
            0
        }
    }

    fn editor_right(&self) -> i32 {
        (self.client_w - self.outline_panel_width()).max(140)
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

    fn wrap_max_px(&self) -> i32 {
        (self.editor_right() - self.gutter_w - 22).max(self.avg_char_w * 8)
    }

    fn compute_line_wraps_with_dc(&self, hdc: HDC, line_str: &str, max_px: i32) -> Vec<(usize, usize)> {
        if line_str.is_empty() {
            return vec![(0, 0)];
        }
        if line_str.len() > 16_384 {
            let mut segs = Vec::new();
            let mut s = 0usize;
            while s < line_str.len() {
                let e = line_str[s..]
                    .char_indices()
                    .nth(120)
                    .map(|(i, _)| s + i)
                    .unwrap_or(line_str.len());
                segs.push((s, e));
                s = e;
            }
            return segs;
        }

        let mut segs = Vec::new();
        let mut seg_start = 0usize;
        let base_chars = ((max_px / (self.avg_char_w / 2).max(3)) as usize + 48).max(64);

        while seg_start < line_str.len() {
            let rem = &line_str[seg_start..];
            let mut chunk_chars = base_chars;
            let (chunk, fit, u16_len, map) = loop {
                let chunk_end = rem
                    .char_indices()
                    .nth(chunk_chars)
                    .map(|(i, _)| i)
                    .unwrap_or(rem.len());
                let ch_slice = &rem[..chunk_end];
                let (exp, map) = expand_tabs_with_u16_map(ch_slice, false);
                let w = to_wide(&exp);
                let u16_len = (w.len() - 1) as i32;
                let mut fit: i32 = 0;
                let mut sz: SIZE = unsafe { std::mem::zeroed() };
                unsafe {
                    GetTextExtentExPointW(hdc, w.as_ptr(), u16_len, max_px, &mut fit, null_mut(), &mut sz);
                }
                if fit == u16_len && chunk_end < rem.len() {
                    chunk_chars *= 2;
                    continue;
                }
                break (ch_slice, fit.max(0) as usize, u16_len as usize, map);
            };

            if fit >= u16_len && chunk.len() == rem.len() {
                segs.push((seg_start, line_str.len()));
                break;
            }

            let mut split_rel = map[fit.min(map.len() - 1)];
            if split_rel == 0 {
                split_rel = rem.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            } else if split_rel < chunk.len() {
                let cb = chunk.as_bytes();
                let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
                if is_word(cb[split_rel - 1]) && is_word(cb[split_rel]) {
                    let min_keep = split_rel / 3;
                    let mut bp = split_rel;
                    while bp > min_keep {
                        if !chunk.is_char_boundary(bp) {
                            bp -= 1;
                            continue;
                        }
                        let prev = cb[bp - 1];
                        if matches!(prev, b' ' | b'\t' | b'-' | b'/' | b',' | b';' | b'.') || prev >= 0x80 {
                            split_rel = bp;
                            break;
                        }
                        bp -= 1;
                    }
                }
            }
            segs.push((seg_start, seg_start + split_rel));
            seg_start += split_rel;
        }
        segs
    }

    fn rebuild_visual_prefix(&mut self) {
        self.visual_prefix.clear();
        self.visual_prefix.reserve(self.line_wraps.len() + 1);
        let mut acc = 0usize;
        self.visual_prefix.push(0);
        for segs in &self.line_wraps {
            acc += segs.len().max(1);
            self.visual_prefix.push(acc);
        }
    }

    fn rebuild_all_wraps(&mut self) {
        self.doc.wrap_patches.clear();
        self.line_wraps.clear();
        if !self.wrap_active() {
            self.visual_prefix.clear();
            return;
        }
        let max_px = self.wrap_max_px();
        let count = self.doc.line_count();
        self.line_wraps.reserve(count);
        unsafe {
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_regular as _);
            for l in 0..count {
                let s = self.doc.line_str(l);
                self.line_wraps.push(self.compute_line_wraps_with_dc(hdc, s, max_px));
            }
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
        }
        self.rebuild_visual_prefix();
    }

    fn sync_wraps_after_edit(&mut self) {
        let patches: Vec<(usize, usize, usize)> = self.doc.wrap_patches.drain(..).collect();
        if !self.wrap_active() {
            return;
        }
        if patches.is_empty() && self.line_wraps.len() == self.doc.line_count() {
            return;
        }
        if self.line_wraps.is_empty() {
            self.rebuild_all_wraps();
            return;
        }
        let max_px = self.wrap_max_px();
        unsafe {
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_regular as _);
            for (start_line, old_count, new_count) in patches {
                if start_line > self.line_wraps.len() || start_line + old_count > self.line_wraps.len() {
                    SelectObject(hdc, old);
                    ReleaseDC(self.hwnd, hdc);
                    self.rebuild_all_wraps();
                    return;
                }
                let mut rep_wraps = Vec::with_capacity(new_count);
                for l in start_line..(start_line + new_count) {
                    if l >= self.doc.line_count() {
                        break;
                    }
                    let s = self.doc.line_str(l);
                    rep_wraps.push(self.compute_line_wraps_with_dc(hdc, s, max_px));
                }
                self.line_wraps.splice(start_line..(start_line + old_count), rep_wraps);
            }
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
        }
        if self.line_wraps.len() != self.doc.line_count() {
            self.rebuild_all_wraps();
        } else {
            self.rebuild_visual_prefix();
        }
    }

    fn total_visual_lines(&self) -> usize {
        if self.wrap_active() {
            self.visual_prefix.last().copied().unwrap_or(1).max(1)
        } else {
            self.doc.line_count().max(1)
        }
    }

    fn get_visual_seg(&self, vrow: usize) -> Option<VisualSeg> {
        if self.wrap_active() {
            let total = self.total_visual_lines();
            if vrow >= total || self.line_wraps.is_empty() {
                return None;
            }
            let line = self.visual_prefix.partition_point(|&p| p <= vrow).saturating_sub(1);
            let seg_idx = vrow - self.visual_prefix[line];
            let (bs, be) = self.line_wraps.get(line)?.get(seg_idx).copied()?;
            Some(VisualSeg { line, b_start: bs, b_end: be })
        } else if vrow < self.doc.line_count() {
            let (s, e) = self.doc.line_bounds(vrow);
            Some(VisualSeg { line: vrow, b_start: 0, b_end: e - s })
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
        let base = *self.visual_prefix.get(line).unwrap_or(&0);
        if let Some(segs) = self.line_wraps.get(line) {
            for (idx, &(_, be)) in segs.iter().enumerate() {
                if rel <= be {
                    return base + idx;
                }
            }
            return base + segs.len().saturating_sub(1);
        }
        base
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
                let edit_w = (self.editor_right() - self.gutter_w).max(50);
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
            let edit_w = (self.editor_right() - self.gutter_w - 24).max(40);
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
            let seg = self.get_visual_seg(vrow);
            self.doc.line_starts[line] + seg.map(|s| s.b_start).unwrap_or(0)
        } else {
            self.doc.line_starts[line]
        };
        let end = byte_pos.max(seg_start).min(self.doc.text.len());
        let slice = &self.doc.text[seg_start..end];
        if slice.len() > 8192 {
            return (slice.len() as i32) * self.avg_char_w;
        }
        let (expanded, _) = expand_tabs_col(slice, 0, false);
        let w = to_wide(&expanded);
        unsafe {
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_regular as _);
            let mut sz: SIZE = std::mem::zeroed();
            GetTextExtentPoint32W(hdc, w.as_ptr(), (w.len() - 1) as i32, &mut sz);
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
            sz.cx
        }
    }

    fn hit_test_vrow_x(&self, vrow: usize, target_x: i32) -> (usize, usize, usize) {
        let clamped_vrow = vrow.min(self.total_visual_lines().saturating_sub(1));
        let Some(seg) = self.get_visual_seg(clamped_vrow) else {
            return (0, 0, 0);
        };
        let line_start = self.doc.line_starts[seg.line];
        let seg_str = &self.doc.text[line_start + seg.b_start..line_start + seg.b_end];
        let px = target_x.max(0);
        if seg_str.is_empty() {
            return (line_start + seg.b_start, seg.line, 0);
        }
        if seg_str.len() > 8192 {
            let approx_chars = (px / self.avg_char_w.max(1)) as usize;
            let byte_off = seg_str.char_indices().nth(approx_chars).map(|(i, _)| i).unwrap_or(seg_str.len());
            let abs = line_start + seg.b_start + byte_off;
            let (_, col) = self.doc.byte_to_line_col(abs);
            return (abs, seg.line, col);
        }

        let (expanded, map) = expand_tabs_with_u16_map(seg_str, true);
        let w = to_wide(&expanded);
        let u16_len = (w.len() - 1) as i32;
        let mut fit: i32 = 0;
        let mut sz: SIZE = unsafe { std::mem::zeroed() };
        unsafe {
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_regular as _);
            GetTextExtentExPointW(hdc, w.as_ptr(), u16_len, px, &mut fit, null_mut(), &mut sz);
            let mut best = fit.max(0) as usize;
            if best < u16_len as usize {
                let mut s1: SIZE = std::mem::zeroed();
                let mut s2: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(hdc, w.as_ptr(), best as i32, &mut s1);
                let next_u16 = if w[best] >= 0xD800 && w[best] <= 0xDBFF && best + 1 < u16_len as usize {
                    best + 2
                } else {
                    best + 1
                };
                GetTextExtentPoint32W(hdc, w.as_ptr(), next_u16 as i32, &mut s2);
                if (px - s1.cx) > (s2.cx - px) {
                    best = next_u16;
                }
            }
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
            let rel_byte = map[best.min(map.len() - 1)];
            let abs = line_start + seg.b_start + rel_byte;
            let (_, col) = self.doc.byte_to_line_col(abs);
            (abs, seg.line, col)
        }
    }

    fn hit_test(&self, mouse_x: i32, mouse_y: i32) -> (usize, usize, usize) {
        let rel_row = (mouse_y.max(0) / self.line_h.max(1)) as usize;
        let vrow = self.scroll_y + rel_row;
        let target_x = mouse_x - self.gutter_w - 8 + self.scroll_x;
        self.hit_test_vrow_x(vrow, target_x)
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
        let x = self.gutter_w + 8 + self.measure_byte_x(head) - self.scroll_x;
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
        let x = (self.editor_right() - FIND_PANEL_W - 14).max(10);
        unsafe {
            MoveWindow(self.find_panel, x, 6, FIND_PANEL_W, h, 1);
            let rep_vis = if self.search.show_replace { SW_SHOW } else { SW_HIDE };
            ShowWindow(self.replace_edit, rep_vis);
            ShowWindow(GetDlgItem(self.find_panel, IDC_BTN_REPLACE), rep_vis);
            ShowWindow(GetDlgItem(self.find_panel, IDC_BTN_REPLACE_ALL), rep_vis);
        }
    }

    pub fn open_file_path(&mut self, path: &std::path::Path) {
        match load_file_auto_encoding(path) {
            Ok((text, enc, eol, size)) => {
                self.doc.set_content(text, Some(path.to_path_buf()), enc, eol, size);
                self.scroll_y = 0;
                self.scroll_x = 0;
                self.update_gutter_width();
                self.rebuild_all_wraps();
                self.update_scrollbars();
                self.update_title();

                if self.cfg.outline_visible {
                    self.outline.refresh_from_doc(&self.doc);
                } else {
                    self.outline.nodes.clear();
                }

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
        let target = if !save_as { self.doc.path.clone() } else { None };
        let path = match target {
            Some(p) => p,
            None => match dialog_save_file(self.hwnd) {
                Some(p) => p,
                None => return false,
            },
        };
        match save_file_with_encoding(&path, &self.doc.text, self.doc.encoding, self.doc.line_ending) {
            Ok(bytes) => {
                let old_lang = self.doc.lang;
                self.doc.path = Some(path.clone());
                self.doc.dirty = false;
                self.doc.disk_bytes = bytes;
                self.doc.lang = detect_lang(Some(&path));
                if old_lang != self.doc.lang {
                    self.doc.rebuild_all_line_states();
                }
                // 非实时刷新：保存时若大纲栏开启则刷新大纲
                if self.cfg.outline_visible {
                    self.outline.refresh_from_doc(&self.doc);
                }
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
// 2. 双缓冲 GDI 虚拟滚动渲染器（编辑区 + 右侧 MD 大纲栏 + 状态栏）
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

fn paint_outline_panel(state: &AppState, mem_dc: HDC, theme: Theme, left: i32, right: i32, bottom: i32) {
    unsafe {
        let bg_br = CreateSolidBrush(theme.outline_bg);
        let rc = RECT { left, top: 0, right, bottom };
        FillRect(mem_dc, &rc, bg_br);
        DeleteObject(bg_br as _);

        let hdr_br = CreateSolidBrush(theme.outline_hdr_bg);
        let hrc = RECT { left, top: 0, right, bottom: OUTLINE_HEADER_H };
        FillRect(mem_dc, &hrc, hdr_br);
        DeleteObject(hdr_br as _);

        let sep_br = CreateSolidBrush(theme.status_border);
        let v_sep = RECT { left, top: 0, right: left + 1, bottom };
        FillRect(mem_dc, &v_sep, sep_br);
        let h_sep = RECT { left, top: OUTLINE_HEADER_H - 1, right, bottom: OUTLINE_HEADER_H };
        FillRect(mem_dc, &h_sep, sep_br);
        DeleteObject(sep_br as _);

        SelectObject(mem_dc, state.hfont_ui_bold as _);
        SetTextColor(mem_dc, theme.outline_fg);
        let title_w = to_wide("Markdown 大纲");
        let mut trc = RECT { left: left + 10, top: 6, right: right - 115, bottom: OUTLINE_HEADER_H - 2 };
        DrawTextW(mem_dc, title_w.as_ptr(), (title_w.len() - 1) as i32, &mut trc, DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);

        // 顶部按钮：[刷新] [全展] [全折] [×]
        SelectObject(mem_dc, state.hfont_ui as _);
        let btn_br = CreateSolidBrush(theme.outline_btn_bg);
        let btns = [
            (right - 112, right - 76, "刷新"),
            (right - 72, right - 42, "展"),
            (right - 38, right - 22, "折"),
            (right - 18, right - 4, "×"),
        ];
        for (bx1, bx2, label) in btns {
            let brc = RECT { left: bx1, top: 5, right: bx2, bottom: OUTLINE_HEADER_H - 5 };
            FillRect(mem_dc, &brc, btn_br);
            let wl = to_wide(label);
            let mut lrc = brc;
            DrawTextW(mem_dc, wl.as_ptr(), (wl.len() - 1) as i32, &mut lrc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
        DeleteObject(btn_br as _);

        let vis = state.outline.visible_indices();
        if vis.is_empty() {
            SetTextColor(mem_dc, theme.outline_muted);
            let empty_w = to_wide("无标题节点\n(点击右上角[刷新]解析)");
            let mut erc = RECT { left: left + 10, top: OUTLINE_HEADER_H + 18, right: right - 10, bottom: bottom - 10 };
            DrawTextW(mem_dc, empty_w.as_ptr(), (empty_w.len() - 1) as i32, &mut erc, DT_CENTER | DT_WORDBREAK);
            return;
        }

        let clip = CreateRectRgn(left + 1, OUTLINE_HEADER_H, right, bottom);
        SelectClipRgn(mem_dc, clip);
        let sel_br = CreateSolidBrush(theme.outline_sel_bg);
        let max_rows = ((bottom - OUTLINE_HEADER_H) / OUTLINE_ROW_H + 1) as usize;

        for r in 0..max_rows {
            let vidx = state.outline.scroll_row + r;
            if vidx >= vis.len() {
                break;
            }
            let node_idx = vis[vidx];
            let node = &state.outline.nodes[node_idx];
            let y = OUTLINE_HEADER_H + (r as i32) * OUTLINE_ROW_H;
            if state.outline.selected_idx == Some(node_idx) {
                let s_rc = RECT { left: left + 2, top: y + 1, right: right - 2, bottom: y + OUTLINE_ROW_H - 1 };
                FillRect(mem_dc, &s_rc, sel_br);
            }

            let indent = 10 + (node.level.saturating_sub(1) as i32) * 14;
            if node.has_children {
                SetTextColor(mem_dc, theme.outline_muted);
                let arrow = to_wide(if node.collapsed { "▸" } else { "▾" });
                let mut arc = RECT { left: left + indent - 2, top: y + 3, right: left + indent + 14, bottom: y + OUTLINE_ROW_H };
                DrawTextW(mem_dc, arrow.as_ptr(), (arrow.len() - 1) as i32, &mut arc, DT_LEFT | DT_SINGLELINE);
            }

            if node.level <= 2 {
                SelectObject(mem_dc, state.hfont_ui_bold as _);
                SetTextColor(mem_dc, if node.level == 1 { theme.h1_fg } else { theme.h2_fg });
            } else {
                SelectObject(mem_dc, state.hfont_ui as _);
                SetTextColor(mem_dc, theme.outline_fg);
            }
            let wt = to_wide(&node.title);
            let mut nrc = RECT {
                left: left + indent + 14,
                top: y + 3,
                right: right - 6,
                bottom: y + OUTLINE_ROW_H,
            };
            DrawTextW(mem_dc, wt.as_ptr(), (wt.len() - 1) as i32, &mut nrc, DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
        }

        DeleteObject(sel_br as _);
        SelectClipRgn(mem_dc, null_mut());
        DeleteObject(clip as _);
    }
}

fn paint_editor(state: &AppState, hdc_win: HDC) {
    let w = state.client_w.max(1);
    let h = state.client_h.max(1);
    let edit_r = state.editor_right();
    let edit_b = h - STATUS_BAR_HEIGHT;
    let theme = get_theme(state.cfg.dark_theme);

    unsafe {
        let mem_dc = CreateCompatibleDC(hdc_win);
        let bmp = CreateCompatibleBitmap(hdc_win, w, h);
        let old_bmp = SelectObject(mem_dc, bmp as _);
        let old_font = SelectObject(mem_dc, state.hfont_regular as _);
        SetBkMode(mem_dc, TRANSPARENT as i32);

        let bg_br = CreateSolidBrush(theme.bg);
        let full_rc = RECT { left: 0, top: 0, right: edit_r, bottom: edit_b };
        FillRect(mem_dc, &full_rc, bg_br);
        DeleteObject(bg_br as _);

        let gut_br = CreateSolidBrush(theme.gutter_bg);
        let gut_rc = RECT { left: 0, top: 0, right: state.gutter_w, bottom: edit_b };
        FillRect(mem_dc, &gut_rc, gut_br);
        DeleteObject(gut_br as _);

        let cur_line = state.doc.line_of_byte(state.doc.primary().head);
        let bracket_pair = if !state.doc.large_mode {
            find_matching_bracket(&state.doc, state.doc.primary().head)
        } else {
            None
        };

        let clip_rgn = CreateRectRgn(state.gutter_w, 0, edit_r, edit_b);
        let vis_rows = state.visible_row_count() + 1;
        let cur_line_br = CreateSolidBrush(theme.cur_line_bg);
        let code_blk_br = CreateSolidBrush(theme.code_block_bg);
        let inline_code_br = CreateSolidBrush(theme.inline_code_bg);
        let quote_bar_br = CreateSolidBrush(theme.acc_fg);
        let sel_br = CreateSolidBrush(theme.sel_bg);
        let match_br = CreateSolidBrush(theme.match_bg);
        let cur_match_br = CreateSolidBrush(theme.cur_match_bg);
        let brk_br = CreateSolidBrush(theme.bracket_bg);
        let caret_br = CreateSolidBrush(theme.caret);
        let hr_pen = CreatePen(PS_SOLID as i32, 1, theme.ws_fg);

        let mut last_drawn_logical = usize::MAX;
        let text_left = state.gutter_w + 8 - state.scroll_x;
        let active_lang = if state.doc.large_mode { Lang::Plain } else { state.doc.lang };

        for r in 0..vis_rows {
            let vrow = state.scroll_y + r;
            let Some(seg) = state.get_visual_seg(vrow) else {
                break;
            };
            let y = (r as i32) * state.line_h;
            if y >= edit_b {
                break;
            }

            let line_state = state.doc.line_states.get(seg.line).copied().unwrap_or(LineState::Normal);
            let full_line = state.doc.line_str(seg.line);
            let (tokens, line_attr) = if full_line.len() <= 8192 {
                highlight_line_with_state(active_lang, full_line, line_state)
            } else {
                (Vec::new(), LineAttr::default())
            };

            if line_attr.is_code_block {
                let cb_rc = RECT { left: state.gutter_w, top: y, right: edit_r, bottom: y + state.line_h };
                FillRect(mem_dc, &cb_rc, code_blk_br);
            }
            if seg.line == cur_line {
                let row_rc = RECT { left: state.gutter_w, top: y, right: edit_r, bottom: y + state.line_h };
                FillRect(mem_dc, &row_rc, cur_line_br);
            }
            if line_attr.is_quote {
                let q_rc = RECT { left: state.gutter_w + 2, top: y + 1, right: state.gutter_w + 5, bottom: y + state.line_h - 1 };
                FillRect(mem_dc, &q_rc, quote_bar_br);
            }

            if state.cfg.show_line_numbers && seg.line != last_drawn_logical {
                SelectObject(mem_dc, state.hfont_regular as _);
                SetTextColor(mem_dc, if seg.line == cur_line { theme.text_fg } else { theme.gutter_fg });
                let num_s = to_wide(&(seg.line + 1).to_string());
                let mut nrc = RECT { left: 2, top: y + 2, right: state.gutter_w - 8, bottom: y + state.line_h };
                DrawTextW(mem_dc, num_s.as_ptr(), (num_s.len() - 1) as i32, &mut nrc, DT_RIGHT | DT_SINGLELINE);
            }
            last_drawn_logical = seg.line;

            SelectClipRgn(mem_dc, clip_rgn);

            if line_attr.is_hr && seg.b_start == 0 {
                let old_pen = SelectObject(mem_dc, hr_pen as _);
                let hy = y + state.line_h / 2;
                MoveToEx(mem_dc, state.gutter_w + 8, hy, null_mut());
                LineTo(mem_dc, edit_r - 12, hy);
                SelectObject(mem_dc, old_pen);
            }

            let line_abs_start = state.doc.line_starts[seg.line];
            let seg_abs_start = line_abs_start + seg.b_start;
            let seg_abs_end = line_abs_start + seg.b_end;
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
                let (exp, _) = expand_tabs_col(sub, 0, false);
                let ew = to_wide(&exp);
                let mut sz: SIZE = std::mem::zeroed();
                SelectObject(mem_dc, state.hfont_regular as _);
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
                    FillRect(mem_dc, &mrc, if state.search.current_idx == Some(idx) { cur_match_br } else { match_br });
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
                    let xs = seg_x(cs.max(seg_abs_start));
                    let mut xe = seg_x(ce.min(seg_abs_end));
                    if ce > seg_abs_end {
                        xe += state.avg_char_w / 2;
                    }
                    let src = RECT { left: xs, top: y, right: xe.max(xs + 2), bottom: y + state.line_h };
                    FillRect(mem_dc, &src, sel_br);
                }
            }

            let mut cur_x = text_left;
            let mut cur_col = 0usize;
            let mut pos = 0usize;
            let draw_chunk = |chunk: &str, color: u32, font: HFONT, inline_bg: bool, x_acc: &mut i32, col_acc: &mut usize| {
                if chunk.is_empty() {
                    return;
                }
                let (shown, next_col) = expand_tabs_col(chunk, *col_acc, state.cfg.show_whitespace);
                *col_acc = next_col;
                let wtxt = to_wide(&shown);
                let clen = (wtxt.len() - 1) as i32;
                SelectObject(mem_dc, font as _);
                let mut sz: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(mem_dc, wtxt.as_ptr(), clen, &mut sz);
                if inline_bg {
                    let irc = RECT { left: *x_acc, top: y + 2, right: *x_acc + sz.cx, bottom: y + state.line_h - 2 };
                    FillRect(mem_dc, &irc, inline_code_br);
                }
                if state.cfg.show_whitespace && chunk.chars().all(|ch| ch == ' ' || ch == '\t') {
                    SetTextColor(mem_dc, theme.ws_fg);
                } else {
                    SetTextColor(mem_dc, color);
                }
                TextOutW(mem_dc, *x_acc, y + 3, wtxt.as_ptr(), clen);
                *x_acc += sz.cx;
            };

            let seg_end_rel = seg.b_start + draw_slice.len();
            for (ts, te, kind) in tokens {
                if te <= seg.b_start || ts >= seg_end_rel {
                    continue;
                }
                let ls = ts.saturating_sub(seg.b_start);
                let le = te.min(seg_end_rel) - seg.b_start;
                if ls > pos {
                    draw_chunk(&draw_slice[pos..ls], theme.text_fg, state.hfont_regular, false, &mut cur_x, &mut cur_col);
                }
                let (tcol, tfont, has_bg) = match kind {
                    TokenKind::Keyword => (theme.kw_fg, state.hfont_regular, false),
                    TokenKind::Heading1 => (theme.h1_fg, state.hfont_bold, false),
                    TokenKind::Heading2 => (theme.h2_fg, state.hfont_bold, false),
                    TokenKind::StringLit => (theme.str_fg, state.hfont_regular, false),
                    TokenKind::Comment => (theme.cmt_fg, state.hfont_regular, false),
                    TokenKind::Number => (theme.num_fg, state.hfont_regular, false),
                    TokenKind::Accent => (theme.acc_fg, state.hfont_regular, false),
                    TokenKind::Bold => (theme.bold_fg, state.hfont_bold, false),
                    TokenKind::Italic => (theme.italic_fg, state.hfont_italic, false),
                    TokenKind::Strike => (theme.strike_fg, state.hfont_strike, false),
                    TokenKind::InlineCode => (theme.code_fg, state.hfont_regular, !line_attr.is_code_block),
                    TokenKind::AlertTag => (theme.alert_fg, state.hfont_bold, true),
                    TokenKind::TaskDone => (theme.task_done_fg, state.hfont_bold, false),
                    TokenKind::TaskTodo => (theme.task_todo_fg, state.hfont_bold, false),
                };
                if le > ls {
                    draw_chunk(&draw_slice[ls..le], tcol, tfont, has_bg, &mut cur_x, &mut cur_col);
                }
                pos = pos.max(le);
            }
            if pos < draw_slice.len() {
                draw_chunk(&draw_slice[pos..], theme.text_fg, state.hfont_regular, false, &mut cur_x, &mut cur_col);
            }

            for c in &state.doc.cursors {
                let in_seg = if seg.b_end == full_line.len() {
                    c.head >= seg_abs_start && c.head <= seg_abs_end
                } else {
                    c.head >= seg_abs_start && c.head < seg_abs_end
                };
                if in_seg {
                    let cx = seg_x(c.head);
                    let crc = RECT { left: cx, top: y + 2, right: cx + 2, bottom: y + state.line_h - 2 };
                    FillRect(mem_dc, &crc, caret_br);
                }
            }

            SelectClipRgn(mem_dc, null_mut());
        }

        DeleteObject(clip_rgn as _);
        DeleteObject(cur_line_br as _);
        DeleteObject(code_blk_br as _);
        DeleteObject(inline_code_br as _);
        DeleteObject(quote_bar_br as _);
        DeleteObject(sel_br as _);
        DeleteObject(match_br as _);
        DeleteObject(cur_match_br as _);
        DeleteObject(brk_br as _);
        DeleteObject(caret_br as _);
        DeleteObject(hr_pen as _);

        if edit_r < w {
            paint_outline_panel(state, mem_dc, theme, edit_r, w, edit_b);
        }

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
        let mode_tag = if state.doc.lang == Lang::Markdown { "Markdown/GFM" } else { "代码高亮" };
        let right_info = format!(
            "字数 {}   |   {}   |   {}   |   {}   |   {}%  ",
            state.doc.total_chars,
            mode_tag,
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
    let filter: Vec<u16> = "Markdown 与文本 (*.md;*.markdown;*.txt;*.log)\0*.md;*.markdown;*.txt;*.log\0所有文件 (*.*)\0*.*\0\0"
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
    let filter: Vec<u16> = "Markdown 文档 (*.md)\0*.md\0文本文档 (*.txt)\0*.txt\0所有文件 (*.*)\0*.*\0\0"
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
// 4. 菜单状态同步、悬浮查找面板、转到行弹窗与快捷键分发
// ============================================================================

fn sync_menu_checks(state: &AppState, hmenu: HMENU) {
    let chk = |id: usize, on: bool| unsafe {
        CheckMenuItem(hmenu, id as u32, MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED });
    };
    chk(IDM_VIEW_WRAP, state.cfg.word_wrap);
    chk(IDM_VIEW_LINENUM, state.cfg.show_line_numbers);
    chk(IDM_VIEW_WHITESPACE, state.cfg.show_whitespace);
    chk(IDM_VIEW_OUTLINE, state.cfg.outline_visible);
    chk(IDM_VIEW_THEME, state.cfg.dark_theme);
    chk(IDM_VIEW_TOPMOST, state.cfg.always_on_top);
    chk(IDM_EOL_CRLF, state.doc.line_ending == LineEnding::Crlf);
    chk(IDM_EOL_LF, state.doc.line_ending == LineEnding::Lf);
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
                        st.sync_wraps_after_edit();
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
                    st.sync_wraps_after_edit();
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
                state.scroll_y = 0;
                state.scroll_x = 0;
                state.update_gutter_width();
                state.rebuild_all_wraps();
                state.update_scrollbars();
                if state.cfg.outline_visible {
                    state.outline.refresh_from_doc(&state.doc);
                } else {
                    state.outline.nodes.clear();
                }
                state.update_title();
                state.invalidate();
            }
        }
        IDM_FILE_OPEN => {
            if state.confirm_discard() {
                if let Some(p) = dialog_open_file(state.hwnd) {
                    state.open_file_path(&p);
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
                    state.open_file_path(&p);
                }
            } else if state.cfg.outline_visible {
                state.outline.refresh_from_doc(&state.doc);
                state.invalidate();
            }
        }
        IDM_FILE_EXIT => unsafe {
            PostMessageW(state.hwnd, WM_CLOSE, 0, 0);
        },

        IDM_EDIT_UNDO => {
            if state.doc.undo() {
                state.update_gutter_width();
                state.sync_wraps_after_edit();
                state.ensure_caret_visible();
                state.update_title();
                state.invalidate();
            }
        }
        IDM_EDIT_REDO => {
            if state.doc.redo() {
                state.update_gutter_width();
                state.sync_wraps_after_edit();
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
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_PASTE => {
            if let Some(txt) = clipboard_paste(state.hwnd) {
                state.doc.insert_text(&txt);
                state.update_gutter_width();
                state.sync_wraps_after_edit();
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
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_DEL_LINE => {
            state.doc.delete_current_line();
            state.update_gutter_width();
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_MOVE_UP => {
            state.doc.move_lines(true);
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_MOVE_DOWN => {
            state.doc.move_lines(false);
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_COMMENT => {
            state.doc.toggle_comment(state.doc.lang);
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
        }
        IDM_EDIT_UPPER | IDM_EDIT_LOWER => {
            state.doc.transform_case(id == IDM_EDIT_UPPER);
            state.sync_wraps_after_edit();
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
            state.rebuild_all_wraps();
            state.ensure_caret_visible();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_LINENUM => {
            state.cfg.show_line_numbers = !state.cfg.show_line_numbers;
            state.update_gutter_width();
            state.rebuild_all_wraps();
            state.update_scrollbars();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_WHITESPACE => {
            state.cfg.show_whitespace = !state.cfg.show_whitespace;
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_OUTLINE => {
            state.cfg.outline_visible = !state.cfg.outline_visible;
            if state.cfg.outline_visible {
                state.outline.refresh_from_doc(&state.doc);
            }
            state.rebuild_all_wraps();
            state.update_scrollbars();
            if state.search.visible {
                state.layout_find_panel();
            }
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_OUTLINE_REFRESH => {
            if !state.cfg.outline_visible {
                state.cfg.outline_visible = true;
                state.rebuild_all_wraps();
                state.update_scrollbars();
                if state.search.visible {
                    state.layout_find_panel();
                }
                save_config(&state.cfg);
            }
            state.outline.refresh_from_doc(&state.doc);
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

fn handle_outline_click(state: &mut AppState, x: i32, y: i32) {
    let left = state.editor_right();
    let right = state.client_w;
    if y < OUTLINE_HEADER_H {
        if x >= right - 112 && x <= right - 76 && y >= 4 && y <= OUTLINE_HEADER_H - 4 {
            state.outline.refresh_from_doc(&state.doc);
            state.invalidate();
        } else if x >= right - 72 && x <= right - 42 && y >= 4 && y <= OUTLINE_HEADER_H - 4 {
            state.outline.set_all_collapsed(false);
            state.invalidate();
        } else if x >= right - 38 && x <= right - 22 && y >= 4 && y <= OUTLINE_HEADER_H - 4 {
            state.outline.set_all_collapsed(true);
            state.invalidate();
        } else if x >= right - 18 && x <= right - 4 && y >= 4 && y <= OUTLINE_HEADER_H - 4 {
            handle_command(state, IDM_VIEW_OUTLINE);
        }
        return;
    }

    let vis = state.outline.visible_indices();
    let row = state.outline.scroll_row + ((y - OUTLINE_HEADER_H) / OUTLINE_ROW_H) as usize;
    if row >= vis.len() {
        return;
    }
    let node_idx = vis[row];
    let node = state.outline.nodes[node_idx].clone();
    let indent = 10 + (node.level.saturating_sub(1) as i32) * 14;
    if node.has_children && x >= left + indent - 4 && x <= left + indent + 16 {
        state.outline.toggle_collapse(node_idx);
        state.invalidate();
        return;
    }

    state.outline.selected_idx = Some(node_idx);
    state.doc.cursors = vec![Cursor::collapsed(node.byte_pos)];
    let target_vrow = state.byte_to_visual_row(node.byte_pos);
    let max_y = state.total_visual_lines().saturating_sub(1);
    state.scroll_y = target_vrow.saturating_sub(2).min(max_y);
    state.update_scrollbars();
    unsafe {
        SetFocus(state.hwnd);
    }
    state.invalidate();
}

fn handle_keydown(state: &mut AppState, vk: u16) -> bool {
    let ctrl = key_down(VK_CONTROL);
    let shift = key_down(VK_SHIFT);
    let alt = key_down(VK_MENU);

    if ctrl && !alt {
        match vk as i32 {
            0x4E => { handle_command(state, IDM_FILE_NEW); return true; }
            0x4F if shift => { handle_command(state, IDM_VIEW_OUTLINE); return true; }
            0x4F => { handle_command(state, IDM_FILE_OPEN); return true; }
            0x4C if shift => { handle_command(state, IDM_VIEW_LINENUM); return true; }
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
            if state.wrap_active() {
                let vrow = state.byte_to_visual_row(head);
                let x = state.measure_byte_x(head);
                let target_vrow = vrow.saturating_sub(1);
                let (pos, _, _) = state.hit_test_vrow_x(target_vrow, x);
                move_cursor(state, pos);
            } else {
                let nl = line.saturating_sub(1);
                move_cursor(state, state.doc.line_col_to_byte(nl, col));
            }
            true
        }
        VK_DOWN => {
            if state.wrap_active() {
                let vrow = state.byte_to_visual_row(head);
                let x = state.measure_byte_x(head);
                let target_vrow = (vrow + 1).min(state.total_visual_lines().saturating_sub(1));
                let (pos, _, _) = state.hit_test_vrow_x(target_vrow, x);
                move_cursor(state, pos);
            } else {
                let nl = (line + 1).min(state.doc.line_count().saturating_sub(1));
                move_cursor(state, state.doc.line_col_to_byte(nl, col));
            }
            true
        }
        VK_HOME => {
            let target = if ctrl {
                0
            } else if state.wrap_active() {
                let vrow = state.byte_to_visual_row(head);
                let seg_start = state
                    .get_visual_seg(vrow)
                    .map(|s| state.doc.line_starts[s.line] + s.b_start)
                    .unwrap_or_else(|| state.doc.line_bounds(line).0);
                if head == seg_start {
                    state.doc.line_bounds(line).0
                } else {
                    seg_start
                }
            } else {
                state.doc.line_bounds(line).0
            };
            move_cursor(state, target);
            true
        }
        VK_END => {
            let target = if ctrl {
                state.doc.text.len()
            } else if state.wrap_active() {
                let vrow = state.byte_to_visual_row(head);
                state
                    .get_visual_seg(vrow)
                    .map(|s| state.doc.line_starts[s.line] + s.b_end)
                    .unwrap_or_else(|| state.doc.line_bounds(line).1)
            } else {
                state.doc.line_bounds(line).1
            };
            move_cursor(state, target);
            true
        }
        VK_PRIOR => {
            let step = state.visible_row_count();
            if state.wrap_active() {
                let vrow = state.byte_to_visual_row(head).saturating_sub(step);
                let x = state.measure_byte_x(head);
                let (pos, _, _) = state.hit_test_vrow_x(vrow, x);
                move_cursor(state, pos);
            } else {
                let nl = line.saturating_sub(step);
                move_cursor(state, state.doc.line_col_to_byte(nl, col));
            }
            true
        }
        VK_NEXT => {
            let step = state.visible_row_count();
            if state.wrap_active() {
                let vrow = (state.byte_to_visual_row(head) + step).min(state.total_visual_lines().saturating_sub(1));
                let x = state.measure_byte_x(head);
                let (pos, _, _) = state.hit_test_vrow_x(vrow, x);
                move_cursor(state, pos);
            } else {
                let nl = (line + step).min(state.doc.line_count().saturating_sub(1));
                move_cursor(state, state.doc.line_col_to_byte(nl, col));
            }
            true
        }
        VK_BACK => {
            state.doc.backspace();
            state.update_gutter_width();
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
            true
        }
        VK_DELETE => {
            state.doc.delete_forward();
            state.update_gutter_width();
            state.sync_wraps_after_edit();
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
            st.rebuild_all_wraps();
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
                let edit_r = st.editor_right();
                if st.cfg.outline_visible && pt.y < st.client_h - STATUS_BAR_HEIGHT && (pt.x - edit_r).abs() <= 4 {
                    SetCursor(LoadCursorW(null_mut(), IDC_SIZEWE));
                } else if pt.x >= st.gutter_w && pt.x < edit_r && pt.y < st.client_h - STATUS_BAR_HEIGHT {
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
                st.doc.insert_text("\t");
            } else if code >= 0x20 {
                if let Some(ch) = char::from_u32(code) {
                    let mut buf = [0u8; 4];
                    st.doc.insert_text(ch.encode_utf8(&mut buf));
                }
            } else {
                return 0;
            }
            st.update_gutter_width();
            st.sync_wraps_after_edit();
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
            let mx = get_x_lparam(lp);
            let my = get_y_lparam(lp);
            let edit_r = st.editor_right();
            if st.cfg.outline_visible && my < st.client_h - STATUS_BAR_HEIGHT && (mx - edit_r).abs() <= 4 {
                SetCapture(hwnd);
                st.resizing_outline = true;
                return 0;
            }
            if st.cfg.outline_visible && mx >= edit_r && my < st.client_h - STATUS_BAR_HEIGHT {
                handle_outline_click(st, mx, my);
                return 0;
            }

            SetFocus(hwnd);
            SetCapture(hwnd);
            st.dragging = true;
            let (pos, line, col) = st.hit_test(mx, my);
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
            let mx = get_x_lparam(lp);
            let my = get_y_lparam(lp);
            if st.resizing_outline {
                let new_w = (st.client_w - mx).clamp(180, (st.client_w / 2).max(180));
                if new_w != st.cfg.outline_width {
                    st.cfg.outline_width = new_w;
                    st.rebuild_all_wraps();
                    st.update_scrollbars();
                    if st.search.visible {
                        st.layout_find_panel();
                    }
                    st.invalidate();
                }
                return 0;
            }
            if st.dragging {
                let (pos, line, col) = st.hit_test(mx, my);
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
            if st.resizing_outline {
                st.resizing_outline = false;
                ReleaseCapture();
                save_config(&st.cfg);
            }
            if st.dragging {
                st.dragging = false;
                st.rect_selecting = false;
                ReleaseCapture();
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp >> 16) as i16) as i32;
            let mut pt = POINT { x: get_x_lparam(lp), y: get_y_lparam(lp) };
            ScreenToClient(hwnd, &mut pt);
            if st.cfg.outline_visible && pt.x >= st.editor_right() && pt.y < st.client_h - STATUS_BAR_HEIGHT {
                let vis_len = st.outline.visible_indices().len();
                let page_rows = ((st.client_h - STATUS_BAR_HEIGHT - OUTLINE_HEADER_H) / OUTLINE_ROW_H).max(1) as usize;
                let max_row = vis_len.saturating_sub(page_rows);
                let step = if delta > 0 { -2isize } else { 2isize };
                st.outline.scroll_row = (st.outline.scroll_row as isize + step).clamp(0, max_row as isize) as usize;
                st.invalidate();
                return 0;
            }
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
                st.open_file_path(&p);
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
            for f in [st.hfont_regular, st.hfont_bold, st.hfont_italic, st.hfont_strike, st.hfont_ui, st.hfont_ui_bold] {
                if !f.is_null() {
                    DeleteObject(f as _);
                }
            }
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
