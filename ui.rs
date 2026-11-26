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
use crate::render::{
    apply_titlebar_theme, compute_line_wraps_with_dc, expand_tabs_col, expand_tabs_with_u16_map,
    get_theme, paint_editor, VisualSeg, OUTLINE_HEADER_H, OUTLINE_ROW_H, STATUS_BAR_HEIGHT,
};
use crate::{
    IDM_EDIT_COMMENT, IDM_EDIT_COPY, IDM_EDIT_CUT, IDM_EDIT_DELETE, IDM_EDIT_DEL_LINE,
    IDM_EDIT_DUP_LINE, IDM_EDIT_LOWER, IDM_EDIT_MOVE_DOWN, IDM_EDIT_MOVE_UP, IDM_EDIT_PASTE,
    IDM_EDIT_REDO, IDM_EDIT_SELECT_ALL, IDM_EDIT_UNDO, IDM_EDIT_UPPER, IDM_EOL_CRLF, IDM_EOL_LF,
    IDM_FILE_EXIT, IDM_FILE_NEW, IDM_FILE_OPEN, IDM_FILE_RELOAD, IDM_FILE_SAVE, IDM_FILE_SAVEAS,
    IDM_OUTLINE_COLLAPSE_ALL, IDM_OUTLINE_EXPAND_ALL, IDM_SEARCH_FIND, IDM_SEARCH_GOTO,
    IDM_SEARCH_NEXT, IDM_SEARCH_PREV, IDM_SEARCH_REPLACE, IDM_VIEW_FONT, IDM_VIEW_LINENUM,
    IDM_VIEW_OUTLINE, IDM_VIEW_OUTLINE_REFRESH, IDM_VIEW_THEME_DARK, IDM_VIEW_THEME_LIGHT,
    IDM_VIEW_TOPMOST, IDM_VIEW_WHITESPACE, IDM_VIEW_WRAP, IDM_VIEW_ZOOM_IN, IDM_VIEW_ZOOM_OUT,
    IDM_VIEW_ZOOM_RESET,
};

const FIND_PANEL_W: i32 = 430;
const FIND_PANEL_H: i32 = 108;
const CF_UNICODETEXT_ID: u32 = 13;
const SS_CENTER: u32 = 1;

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

pub struct AppState {
    pub hwnd: HWND,
    pub hfont_regular: HFONT,
    pub hfont_bold: HFONT,
    pub hfont_italic: HFONT,
    pub hfont_strike: HFONT,
    pub hfont_ui: HFONT,
    pub hfont_ui_bold: HFONT,
    pub find_bg_brush: HBRUSH,
    pub find_edit_brush: HBRUSH,
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
    pub last_wrap_px: i32,
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

    pub fn rebuild_theme_brushes(&mut self) {
        let theme = get_theme(self.cfg.dark_theme);
        unsafe {
            if !self.find_bg_brush.is_null() {
                DeleteObject(self.find_bg_brush as _);
            }
            if !self.find_edit_brush.is_null() {
                DeleteObject(self.find_edit_brush as _);
            }
            self.find_bg_brush = CreateSolidBrush(theme.outline_hdr_bg);
            self.find_edit_brush = CreateSolidBrush(theme.bg);
        }
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

    pub fn update_gutter_width(&mut self) {
        if !self.cfg.show_line_numbers {
            self.gutter_w = 14;
            return;
        }
        let digits = self.doc.line_count().max(1).to_string().len().max(3);
        self.gutter_w = (digits as i32 * self.avg_char_w) + 22;
    }

    pub fn outline_panel_width(&self) -> i32 {
        if self.cfg.outline_visible && self.client_w > 460 {
            self.cfg.outline_width.clamp(180, (self.client_w / 2).max(180))
        } else {
            0
        }
    }

    pub fn editor_right(&self) -> i32 {
        (self.client_w - self.outline_panel_width()).max(140)
    }

    pub fn editor_height(&self) -> i32 {
        (self.client_h - STATUS_BAR_HEIGHT).max(self.line_h)
    }

    pub fn visible_row_count(&self) -> usize {
        ((self.editor_height() / self.line_h.max(1)).max(1)) as usize
    }

    pub fn wrap_active(&self) -> bool {
        self.cfg.word_wrap && !self.doc.large_mode
    }

    fn wrap_max_px(&self) -> i32 {
        (self.editor_right() - self.gutter_w - 30).max(self.avg_char_w * 6)
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

    pub fn rebuild_all_wraps(&mut self) {
        self.doc.wrap_patches.clear();
        self.line_wraps.clear();
        if !self.wrap_active() {
            self.visual_prefix.clear();
            self.last_wrap_px = 0;
            return;
        }
        if self.client_w <= 60 {
            return;
        }
        let max_px = self.wrap_max_px();
        self.last_wrap_px = max_px;
        let count = self.doc.line_count();
        self.line_wraps.reserve(count);
        let is_md = self.doc.lang == Lang::Markdown;
        unsafe {
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.hfont_regular as _);
            for l in 0..count {
                let s = self.doc.line_str(l);
                self.line_wraps.push(compute_line_wraps_with_dc(
                    hdc,
                    s,
                    max_px,
                    self.avg_char_w,
                    self.cfg.show_whitespace,
                    self.hfont_regular,
                    self.hfont_bold,
                    is_md,
                ));
            }
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
        }
        self.rebuild_visual_prefix();
    }

    pub fn sync_wraps_after_edit(&mut self) {
        let patches: Vec<(usize, usize, usize)> = self.doc.wrap_patches.drain(..).collect();
        if !self.wrap_active() {
            return;
        }
        let max_px = self.wrap_max_px();
        if self.line_wraps.is_empty() || max_px != self.last_wrap_px {
            self.rebuild_all_wraps();
            return;
        }
        if patches.is_empty() && self.line_wraps.len() == self.doc.line_count() {
            return;
        }
        let is_md = self.doc.lang == Lang::Markdown;
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
                    rep_wraps.push(compute_line_wraps_with_dc(
                        hdc,
                        s,
                        max_px,
                        self.avg_char_w,
                        self.cfg.show_whitespace,
                        self.hfont_regular,
                        self.hfont_bold,
                        is_md,
                    ));
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

    pub fn total_visual_lines(&self) -> usize {
        if self.wrap_active() {
            self.visual_prefix.last().copied().unwrap_or(1).max(1)
        } else {
            self.doc.line_count().max(1)
        }
    }

    pub fn get_visual_seg(&self, vrow: usize) -> Option<VisualSeg> {
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

    pub fn byte_to_visual_row(&self, byte_pos: usize) -> usize {
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
        let (expanded, _) = expand_tabs_col(slice, 0, self.cfg.show_whitespace);
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

        let (expanded, map) = expand_tabs_with_u16_map(seg_str, self.cfg.show_whitespace, true);
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
// 剪贴板与系统文件/字体对话框
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
// 右键上下文菜单（编辑区 + 大纲栏）
// ============================================================================

fn show_editor_context_menu(state: &AppState, mx: i32, my: i32) {
    unsafe {
        let menu = CreatePopupMenu();
        let add = |id: usize, label: &str, enabled: bool, checked: bool| {
            let w = to_wide(label);
            let mut flags = MF_STRING;
            if !enabled {
                flags |= MF_GRAYED;
            }
            if checked {
                flags |= MF_CHECKED;
            }
            AppendMenuW(menu, flags, id, w.as_ptr());
        };
        let has_sel = state.doc.has_any_selection();
        let can_paste = IsClipboardFormatAvailable(CF_UNICODETEXT_ID) != 0;

        add(IDM_EDIT_UNDO, "撤销(&U)\tCtrl+Z", state.doc.can_undo(), false);
        add(IDM_EDIT_REDO, "重做(&R)\tCtrl+Y", state.doc.can_redo(), false);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(IDM_EDIT_CUT, "剪切(&T)\tCtrl+X", !state.doc.text.is_empty(), false);
        add(IDM_EDIT_COPY, "复制(&C)\tCtrl+C", !state.doc.text.is_empty(), false);
        add(IDM_EDIT_PASTE, "粘贴(&P)\tCtrl+V", can_paste, false);
        add(IDM_EDIT_DELETE, "删除(&D)", has_sel, false);
        add(IDM_EDIT_SELECT_ALL, "全选(&A)\tCtrl+A", !state.doc.text.is_empty(), false);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(IDM_EDIT_DUP_LINE, "复制当前行\tCtrl+D", true, false);
        add(IDM_EDIT_DEL_LINE, "删除当前行\tCtrl+Shift+K", true, false);
        add(IDM_EDIT_COMMENT, "切换引用 / 注释\tCtrl+/", true, false);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(IDM_SEARCH_FIND, "查找(&F)...\tCtrl+F", true, false);
        add(IDM_SEARCH_REPLACE, "替换(&H)...\tCtrl+H", true, false);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(IDM_VIEW_WRAP, "自动换行(&W)", true, state.cfg.word_wrap);
        add(IDM_VIEW_LINENUM, "显示行号(&L)", true, state.cfg.show_line_numbers);
        add(IDM_VIEW_OUTLINE, "Markdown 大纲栏(&O)", true, state.cfg.outline_visible);

        let mut pt = POINT { x: mx, y: my };
        ClientToScreen(state.hwnd, &mut pt);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, state.hwnd, null());
        DestroyMenu(menu);
    }
}

fn show_outline_context_menu(state: &AppState, mx: i32, my: i32) {
    unsafe {
        let menu = CreatePopupMenu();
        let add = |id: usize, label: &str| {
            let w = to_wide(label);
            AppendMenuW(menu, MF_STRING, id, w.as_ptr());
        };
        add(IDM_VIEW_OUTLINE_REFRESH, "刷新大纲目录(&R)");
        add(IDM_OUTLINE_EXPAND_ALL, "全部展开(&E)");
        add(IDM_OUTLINE_COLLAPSE_ALL, "全部折叠(&C)");
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(IDM_VIEW_OUTLINE, "关闭大纲栏(&X)");

        let mut pt = POINT { x: mx, y: my };
        ClientToScreen(state.hwnd, &mut pt);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, state.hwnd, null());
        DestroyMenu(menu);
    }
}

// ============================================================================
// 菜单同步、主题化悬浮查找面板、转到行弹窗与指令分发
// ============================================================================

fn sync_menu_checks(state: &AppState, hmenu: HMENU) {
    let chk = |id: usize, on: bool| unsafe {
        CheckMenuItem(hmenu, id as u32, MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED });
    };
    chk(IDM_VIEW_WRAP, state.cfg.word_wrap);
    chk(IDM_VIEW_LINENUM, state.cfg.show_line_numbers);
    chk(IDM_VIEW_WHITESPACE, state.cfg.show_whitespace);
    chk(IDM_VIEW_OUTLINE, state.cfg.outline_visible);
    chk(IDM_VIEW_THEME_DARK, state.cfg.dark_theme);
    chk(IDM_VIEW_THEME_LIGHT, !state.cfg.dark_theme);
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
    if !state_ptr.is_null() {
        let st = &mut *state_ptr;
        let theme = get_theme(st.cfg.dark_theme);
        match msg {
            WM_ERASEBKGND => {
                let hdc = wp as HDC;
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, st.find_bg_brush);
                return 1;
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let hdc = wp as HDC;
                SetTextColor(hdc, theme.outline_fg);
                SetBkColor(hdc, theme.outline_hdr_bg);
                return st.find_bg_brush as isize;
            }
            WM_CTLCOLOREDIT => {
                let hdc = wp as HDC;
                SetTextColor(hdc, theme.text_fg);
                SetBkColor(hdc, theme.bg);
                return st.find_edit_brush as isize;
            }
            WM_COMMAND => {
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
            hbrBackground: null_mut(),
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

fn apply_theme_choice(state: &mut AppState, dark: bool) {
    state.cfg.dark_theme = dark;
    apply_titlebar_theme(state.hwnd, dark);
    state.rebuild_theme_brushes();
    save_config(&state.cfg);
    if !state.find_panel.is_null() {
        unsafe {
            InvalidateRect(state.find_panel, null(), 1);
        }
    }
    state.invalidate();
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
        IDM_EDIT_DELETE => {
            state.doc.delete_forward();
            state.update_gutter_width();
            state.sync_wraps_after_edit();
            state.ensure_caret_visible();
            state.update_title();
            state.invalidate();
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
            state.rebuild_all_wraps();
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
        IDM_OUTLINE_EXPAND_ALL => {
            state.outline.set_all_collapsed(false);
            state.invalidate();
        }
        IDM_OUTLINE_COLLAPSE_ALL => {
            state.outline.set_all_collapsed(true);
            state.invalidate();
        }
        IDM_VIEW_FONT => dialog_choose_font(state),
        IDM_VIEW_THEME_DARK => apply_theme_choice(state, true),
        IDM_VIEW_THEME_LIGHT => apply_theme_choice(state, false),
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
                let (pos, _, _) = state.hit_test_vrow_x(vrow.saturating_sub(1), x);
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
            handle_command(state, IDM_EDIT_DELETE);
            true
        }
        _ => false,
    }
}

// ============================================================================
// Win32 主窗口过程
// ============================================================================

pub unsafe extern "system" fn main_wnd_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let st = &mut *ptr;

    match msg {
        WM_SIZE => {
            let nw = loword(lp as usize) as i32;
            let nh = hiword(lp as usize) as i32;
            if nw > 60 && nh > 60 {
                st.client_w = nw;
                st.client_h = nh;
                st.rebuild_all_wraps();
                st.update_scrollbars();
                if st.search.visible {
                    st.layout_find_panel();
                }
                st.invalidate();
            }
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
        WM_RBUTTONUP => {
            let mx = get_x_lparam(lp);
            let my = get_y_lparam(lp);
            if my >= st.client_h - STATUS_BAR_HEIGHT {
                return 0;
            }
            SetFocus(hwnd);
            if st.cfg.outline_visible && mx >= st.editor_right() {
                show_outline_context_menu(st, mx, my);
            } else {
                let (pos, _, _) = st.hit_test(mx, my);
                let in_sel = st.doc.cursors.iter().any(|c| {
                    let (cs, ce) = c.range();
                    cs < ce && pos >= cs && pos <= ce
                });
                if !in_sel {
                    st.doc.cursors = vec![Cursor::collapsed(pos)];
                    st.invalidate();
                }
                show_editor_context_menu(st, mx, my);
            }
            0
        }
        WM_CONTEXTMENU => {
            // 支持键盘 Shift+F10 / Menu 键唤起右键菜单
            if lp == -1 {
                let head = st.doc.primary().head;
                let vrow = st.byte_to_visual_row(head);
                let y = ((vrow as isize - st.scroll_y as isize) as i32 * st.line_h + st.line_h)
                    .clamp(0, st.client_h - STATUS_BAR_HEIGHT);
                let x = (st.gutter_w + 8 + st.measure_byte_x(head) - st.scroll_x).clamp(0, st.editor_right());
                show_editor_context_menu(st, x, y);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
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
            for b in [st.find_bg_brush, st.find_edit_brush] {
                if !b.is_null() {
                    DeleteObject(b as _);
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
