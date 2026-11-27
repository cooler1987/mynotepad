use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::IsClipboardFormatAvailable;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{EM_SETSEL, SetScrollInfo};
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::engine::*;
use crate::render::{
    apply_titlebar_theme, compute_line_wraps_with_dc, expand_tabs_col, expand_tabs_with_u16_map,
    get_theme, paint_editor, paint_floating_outline, VisualSeg, FOLD_COL_W, OUTLINE_HEADER_H,
    OUTLINE_ROW_H, STATUS_BAR_HEIGHT,
};
use crate::{
    clipboard_copy, clipboard_paste, dialog_choose_font, dialog_open_file, dialog_save_file,
    show_goto_dialog, CF_UNICODETEXT_ID, FIND_PANEL_H, FIND_PANEL_W, IDC_BTN_REPLACE,
    IDC_BTN_REPLACE_ALL, IDM_EDIT_COMMENT, IDM_EDIT_COPY, IDM_EDIT_CUT, IDM_EDIT_DELETE,
    IDM_EDIT_DEL_LINE, IDM_EDIT_DUP_LINE, IDM_EDIT_LOWER, IDM_EDIT_MOVE_DOWN, IDM_EDIT_MOVE_UP,
    IDM_EDIT_PASTE, IDM_EDIT_REDO, IDM_EDIT_SELECT_ALL, IDM_EDIT_UNDO, IDM_EDIT_UPPER,
    IDM_EOL_CRLF, IDM_EOL_LF, IDM_FILE_EXIT, IDM_FILE_NEW, IDM_FILE_OPEN, IDM_FILE_RECENT_BASE,
    IDM_FILE_RECENT_CLEAR, IDM_FILE_RELOAD, IDM_FILE_SAVE, IDM_FILE_SAVEAS,
    IDM_OUTLINE_COLLAPSE_ALL, IDM_OUTLINE_EXPAND_ALL, IDM_SEARCH_FIND, IDM_SEARCH_GOTO,
    IDM_SEARCH_NEXT, IDM_SEARCH_PREV, IDM_SEARCH_REPLACE, IDM_VIEW_FOLD_ALL, IDM_VIEW_FOLD_TOGGLE,
    IDM_VIEW_FONT, IDM_VIEW_LINENUM, IDM_VIEW_OUTLINE, IDM_VIEW_OUTLINE_FLOAT,
    IDM_VIEW_OUTLINE_REFRESH, IDM_VIEW_THEME_DARK, IDM_VIEW_THEME_LIGHT, IDM_VIEW_TOPMOST,
    IDM_VIEW_UNFOLD_ALL, IDM_VIEW_WHITESPACE, IDM_VIEW_WRAP, IDM_VIEW_ZOOM_IN, IDM_VIEW_ZOOM_OUT,
    IDM_VIEW_ZOOM_RESET,
};

#[inline]
fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
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
    pub recent_menu: HMENU,
    pub outline_hwnd: HWND,
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
    pub wrap_max_px_cached: i32,
    pub line_wraps: Vec<Option<Vec<(usize, usize)>>>,
    pub line_row_counts: Vec<usize>,
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
                    DEFAULT_PITCH as u32,
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
        self.reset_wraps_lazy();
    }

    pub fn update_gutter_width(&mut self) {
        if !self.cfg.show_line_numbers {
            self.gutter_w = 6 + FOLD_COL_W;
            return;
        }
        let digits = self.doc.line_count().max(1).to_string().len().max(3);
        self.gutter_w = (digits as i32 * self.avg_char_w) + 16 + FOLD_COL_W;
    }

    fn outline_panel_width(&self) -> i32 {
        if self.cfg.outline_visible && !self.cfg.outline_floating && self.client_w > 460 {
            self.cfg.outline_width.clamp(180, (self.client_w / 2).max(180))
        } else {
            0
        }
    }

    pub fn editor_right(&self) -> i32 {
        (self.client_w - self.outline_panel_width()).max(140)
    }

    fn editor_height(&self) -> i32 {
        (self.client_h - STATUS_BAR_HEIGHT).max(self.line_h)
    }

    pub fn visible_row_count(&self) -> usize {
        ((self.editor_height() / self.line_h.max(1)).max(1)) as usize
    }

    pub fn wrap_active(&self) -> bool {
        self.cfg.word_wrap
    }

    fn wrap_max_px(&self) -> i32 {
        (self.editor_right() - self.gutter_w - 30).max(self.avg_char_w * 6)
    }

    fn rebuild_visual_prefix_from(&mut self, start_line: usize) {
        if start_line == 0 || self.visual_prefix.len() != self.line_row_counts.len() + 1 {
            self.visual_prefix.clear();
            self.visual_prefix.reserve(self.line_row_counts.len() + 1);
            let mut acc = 0usize;
            self.visual_prefix.push(0);
            for &cnt in &self.line_row_counts {
                acc += cnt;
                self.visual_prefix.push(acc);
            }
            return;
        }
        let mut acc = self.visual_prefix[start_line];
        for i in start_line..self.line_row_counts.len() {
            acc += self.line_row_counts[i];
            self.visual_prefix[i + 1] = acc;
        }
    }

    pub fn rebuild_fold_row_counts(&mut self) {
        let count = self.doc.line_count();
        if self.line_wraps.len() != count || self.line_row_counts.len() != count {
            self.reset_wraps_lazy();
            self.update_scrollbars();
            return;
        }
        let max_px = self.wrap_max_px().max(1);
        let est_cols = ((max_px / self.avg_char_w.max(6)).max(8)) as usize;
        for l in 0..count {
            if self.doc.line_hidden.get(l).copied().unwrap_or(false) {
                self.line_row_counts[l] = 0;
            } else if !self.wrap_active() {
                self.line_row_counts[l] = 1;
            } else if let Some(Some(segs)) = self.line_wraps.get(l) {
                self.line_row_counts[l] = segs.len().max(1);
            } else {
                let (s, e) = self.doc.line_bounds(l);
                self.line_row_counts[l] = ((e - s) / est_cols.max(1)).max(1);
            }
        }
        self.rebuild_visual_prefix_from(0);
        self.update_scrollbars();
    }

    pub fn reset_wraps_lazy(&mut self) {
        self.doc.wrap_patches.clear();
        let count = self.doc.line_count();
        if self.client_w <= 60 {
            return;
        }
        let max_px = self.wrap_max_px();
        self.wrap_max_px_cached = if self.wrap_active() { max_px } else { 0 };
        let est_cols = ((max_px / self.avg_char_w.max(6)).max(8)) as usize;

        self.line_wraps.clear();
        self.line_wraps.resize(count, None);
        self.line_row_counts.clear();
        self.line_row_counts.reserve(count);
        for l in 0..count {
            if self.doc.line_hidden.get(l).copied().unwrap_or(false) {
                self.line_row_counts.push(0);
            } else if !self.wrap_active() {
                self.line_row_counts.push(1);
            } else {
                let (s, e) = self.doc.line_bounds(l);
                let est = ((e - s) / est_cols.max(1)).max(1);
                self.line_row_counts.push(est);
            }
        }
        self.rebuild_visual_prefix_from(0);
    }

    fn ensure_line_wrapped_with_dc(&mut self, hdc: HDC, line: usize) -> bool {
        if !self.wrap_active() || line >= self.doc.line_count() {
            return false;
        }
        if self.doc.line_hidden.get(line).copied().unwrap_or(false) {
            return false;
        }
        if self.line_wraps.len() != self.doc.line_count() {
            self.reset_wraps_lazy();
        }
        if self.line_wraps.get(line).is_some_and(|w| w.is_some()) {
            return false;
        }
        let segs = compute_line_wraps_with_dc(
            hdc,
            self.doc.line_str(line),
            self.wrap_max_px_cached,
            self.avg_char_w,
            self.cfg.show_whitespace,
            self.hfont_regular,
            self.hfont_bold,
            self.doc.lang == Lang::Markdown,
        );
        let new_rows = segs.len().max(1);
        let old_rows = self.line_row_counts[line];
        self.line_wraps[line] = Some(segs);
        if new_rows != old_rows {
            self.line_row_counts[line] = new_rows;
            self.rebuild_visual_prefix_from(line);
            return true;
        }
        false
    }

    fn ensure_single_line_wrapped(&mut self, line: usize) {
        if !self.wrap_active() || line >= self.doc.line_count() {
            return;
        }
        if self.line_wraps.get(line).is_some_and(|w| w.is_some()) && self.wrap_max_px_cached == self.wrap_max_px() {
            return;
        }
        if self.wrap_max_px_cached != self.wrap_max_px() {
            self.reset_wraps_lazy();
        }
        unsafe {
            let hdc = GetDC(self.hwnd);
            self.ensure_line_wrapped_with_dc(hdc, line);
            ReleaseDC(self.hwnd, hdc);
        }
    }

    pub fn ensure_viewport_wraps(&mut self) {
        if !self.wrap_active() || self.client_w <= 60 {
            return;
        }
        let cur_max = self.wrap_max_px();
        if self.line_wraps.len() != self.doc.line_count() || self.wrap_max_px_cached != cur_max {
            self.reset_wraps_lazy();
        }
        let vis_rows = self.visible_row_count() + 4;
        let mut changed = false;
        unsafe {
            let hdc = GetDC(self.hwnd);
            for r in 0..vis_rows {
                let vrow = self.scroll_y + r;
                if vrow >= self.total_visual_lines() {
                    break;
                }
                let line = self.visual_prefix.partition_point(|&p| p <= vrow).saturating_sub(1);
                if self.ensure_line_wrapped_with_dc(hdc, line) {
                    changed = true;
                }
            }
            ReleaseDC(self.hwnd, hdc);
        }
        if changed {
            self.update_scrollbars();
        }
    }

    pub fn sync_wraps_after_edit(&mut self) {
        let patches: Vec<(usize, usize, usize)> = self.doc.wrap_patches.drain(..).collect();
        let max_px = self.wrap_max_px();
        if self.line_wraps.len() == 0 || (self.wrap_active() && max_px != self.wrap_max_px_cached) {
            self.reset_wraps_lazy();
            return;
        }
        if patches.is_empty() && self.line_wraps.len() == self.doc.line_count() {
            return;
        }
        if self.doc.line_folded.iter().any(|&f| f) {
            self.reset_wraps_lazy();
            return;
        }
        let est_cols = ((max_px / self.avg_char_w.max(6)).max(8)) as usize;
        let mut min_dirty = usize::MAX;
        unsafe {
            let hdc = GetDC(self.hwnd);
            for (start_line, old_count, new_count) in patches {
                if start_line > self.line_wraps.len() || start_line + old_count > self.line_wraps.len() {
                    ReleaseDC(self.hwnd, hdc);
                    self.reset_wraps_lazy();
                    return;
                }
                min_dirty = min_dirty.min(start_line);
                if !self.wrap_active() {
                    self.line_wraps.splice(start_line..(start_line + old_count), vec![None; new_count]);
                    self.line_row_counts.splice(start_line..(start_line + old_count), vec![1; new_count]);
                } else if new_count <= 24 {
                    let mut rep_wraps = Vec::with_capacity(new_count);
                    let mut rep_counts = Vec::with_capacity(new_count);
                    for l in start_line..(start_line + new_count) {
                        if l >= self.doc.line_count() {
                            break;
                        }
                        let segs = compute_line_wraps_with_dc(
                            hdc,
                            self.doc.line_str(l),
                            max_px,
                            self.avg_char_w,
                            self.cfg.show_whitespace,
                            self.hfont_regular,
                            self.hfont_bold,
                            self.doc.lang == Lang::Markdown,
                        );
                        rep_counts.push(segs.len().max(1));
                        rep_wraps.push(Some(segs));
                    }
                    self.line_wraps.splice(start_line..(start_line + old_count), rep_wraps);
                    self.line_row_counts.splice(start_line..(start_line + old_count), rep_counts);
                } else {
                    let mut rep_counts = Vec::with_capacity(new_count);
                    for l in start_line..(start_line + new_count) {
                        let (s, e) = self.doc.line_bounds(l);
                        rep_counts.push(((e - s) / est_cols.max(1)).max(1));
                    }
                    self.line_wraps.splice(start_line..(start_line + old_count), vec![None; new_count]);
                    self.line_row_counts.splice(start_line..(start_line + old_count), rep_counts);
                }
            }
            ReleaseDC(self.hwnd, hdc);
        }
        if self.line_wraps.len() != self.doc.line_count() {
            self.reset_wraps_lazy();
        } else if min_dirty != usize::MAX {
            self.rebuild_visual_prefix_from(min_dirty);
        }
    }

    pub fn after_doc_edited(&mut self) {
        self.update_gutter_width();
        self.sync_wraps_after_edit();
        self.ensure_caret_visible();
        self.update_title();
        if self.search.visible {
            self.refresh_search_ui();
        }
    }

    pub fn total_visual_lines(&self) -> usize {
        self.visual_prefix.last().copied().unwrap_or(1).max(1)
    }

    pub fn get_visual_seg(&self, vrow: usize) -> Option<VisualSeg> {
        let total = self.total_visual_lines();
        if vrow >= total || self.visual_prefix.len() <= 1 {
            return None;
        }
        let line = self.visual_prefix.partition_point(|&p| p <= vrow).saturating_sub(1);
        if line >= self.doc.line_count() || self.doc.line_hidden.get(line).copied().unwrap_or(false) {
            return None;
        }
        if self.wrap_active() {
            let seg_idx = vrow - self.visual_prefix[line];
            if let Some(Some(segs)) = self.line_wraps.get(line) {
                let (bs, be) = segs.get(seg_idx).copied().unwrap_or_else(|| {
                    let (s, e) = self.doc.line_bounds(line);
                    (0, e - s)
                });
                return Some(VisualSeg { line, b_start: bs, b_end: be });
            }
        }
        let (s, e) = self.doc.line_bounds(line);
        Some(VisualSeg { line, b_start: 0, b_end: e - s })
    }

    pub fn byte_to_visual_row(&mut self, byte_pos: usize) -> usize {
        let line = self.doc.line_of_byte(byte_pos);
        if self.doc.ensure_line_unfolded(line) {
            self.rebuild_fold_row_counts();
        }
        if self.wrap_active() {
            self.ensure_single_line_wrapped(line);
        }
        let base = *self.visual_prefix.get(line).unwrap_or(&0);
        if !self.wrap_active() {
            return base;
        }
        let line_start = self.doc.line_starts[line];
        let rel = byte_pos.saturating_sub(line_start);
        if let Some(Some(segs)) = self.line_wraps.get(line) {
            for (idx, &(_, be)) in segs.iter().enumerate() {
                if rel <= be {
                    return base + idx;
                }
            }
            return base + segs.len().saturating_sub(1);
        }
        base
    }

    pub fn update_title(&self) {
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

    pub fn ensure_caret_visible(&mut self) {
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

    pub fn center_caret_in_view(&mut self) {
        let head = self.doc.primary().head;
        let vrow = self.byte_to_visual_row(head);
        let page = self.visible_row_count();
        let max_y = self.total_visual_lines().saturating_sub(1);
        self.scroll_y = vrow.saturating_sub(page / 2).min(max_y.saturating_sub(page.saturating_sub(1)));

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

    fn measure_byte_x(&mut self, byte_pos: usize) -> i32 {
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

    fn hit_test_vrow_x(&mut self, vrow: usize, target_x: i32) -> (usize, usize, usize) {
        if self.wrap_active() && !self.visual_prefix.is_empty() {
            let tv = vrow.min(self.total_visual_lines().saturating_sub(1));
            let line = self.visual_prefix.partition_point(|&p| p <= tv).saturating_sub(1);
            self.ensure_single_line_wrapped(line);
        }
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

    fn hit_test(&mut self, mouse_x: i32, mouse_y: i32) -> (usize, usize, usize) {
        let rel_row = (mouse_y.max(0) / self.line_h.max(1)) as usize;
        let vrow = self.scroll_y + rel_row;
        let target_x = mouse_x - self.gutter_w - 8 + self.scroll_x;
        self.hit_test_vrow_x(vrow, target_x)
    }

    pub fn invalidate(&self) {
        unsafe {
            InvalidateRect(self.hwnd, null(), 0);
        }
    }

    pub fn invalidate_outline(&self) {
        unsafe {
            if self.cfg.outline_floating && !self.outline_hwnd.is_null() {
                InvalidateRect(self.outline_hwnd, null(), 0);
            } else {
                InvalidateRect(self.hwnd, null(), 0);
            }
        }
    }

    pub fn sync_outline_window(&mut self) {
        unsafe {
            if self.cfg.outline_visible && self.cfg.outline_floating {
                if self.outline_hwnd.is_null() {
                    let hinst = GetModuleHandleW(null());
                    let cls = to_wide("NotepadLiteOutlineFloat");
                    let wc = WNDCLASSEXW {
                        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                        style: CS_DBLCLKS,
                        lpfnWndProc: Some(outline_wnd_proc),
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

                    let mut rc: RECT = std::mem::zeroed();
                    GetWindowRect(self.hwnd, &mut rc);
                    let fw = self.cfg.outline_float_w.clamp(200, 600);
                    let fh = self.cfg.outline_float_h.clamp(220, 900);
                    let (fx, fy) = if self.cfg.outline_x == CW_USEDEFAULT {
                        ((rc.right - fw - 26).max(rc.left + 40), rc.top + 82)
                    } else {
                        (self.cfg.outline_x, self.cfg.outline_y)
                    };
                    let ex = WS_EX_TOOLWINDOW | if self.cfg.always_on_top { WS_EX_TOPMOST } else { 0 };
                    let title = to_wide("Markdown 大纲");
                    let ohwnd = CreateWindowExW(
                        ex,
                        cls.as_ptr(),
                        title.as_ptr(),
                        WS_POPUP | WS_BORDER | WS_CLIPCHILDREN,
                        fx,
                        fy,
                        fw,
                        fh,
                        self.hwnd,
                        null_mut(),
                        hinst,
                        null(),
                    );
                    if !ohwnd.is_null() {
                        SetWindowLongPtrW(ohwnd, GWLP_USERDATA, self as *mut AppState as isize);
                        self.outline_hwnd = ohwnd;
                        ShowWindow(ohwnd, SW_SHOWNOACTIVATE);
                    }
                } else {
                    ShowWindow(self.outline_hwnd, SW_SHOWNOACTIVATE);
                    InvalidateRect(self.outline_hwnd, null(), 0);
                }
            } else if !self.outline_hwnd.is_null() {
                DestroyWindow(self.outline_hwnd);
                self.outline_hwnd = null_mut();
            }
        }
        self.reset_wraps_lazy();
        self.update_scrollbars();
        if self.search.visible {
            self.layout_find_panel();
        }
        self.invalidate();
    }

    fn update_ime_pos(&mut self) {
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

    pub fn update_search_count_label(&self) {
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
    }

    pub fn refresh_search_ui(&mut self) {
        let caret = self.doc.primary().head;
        self.search.recompute(&self.doc.text, caret);
        self.update_search_count_label();
        self.invalidate();
    }

    pub fn jump_to_search_match(&mut self, ms: usize, me: usize) {
        let target_line = self.doc.line_of_byte(ms);
        if self.doc.ensure_line_unfolded(target_line) {
            self.rebuild_fold_row_counts();
        }
        self.doc.cursors = vec![Cursor { anchor: ms, head: me }];
        self.center_caret_in_view();
        self.update_search_count_label();
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

    pub fn hide_find_panel(&mut self) {
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
                self.reset_wraps_lazy();
                self.update_scrollbars();
                self.update_title();

                if self.cfg.outline_visible {
                    self.outline.refresh_from_doc(&self.doc);
                    self.invalidate_outline();
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
            None => match dialog_save_file(self.hwnd, self.doc.path.as_deref()) {
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
                    self.invalidate_outline();
                }
                let pstr = path.to_string_lossy().to_string();
                self.cfg.recent_files.retain(|r| r != &pstr);
                self.cfg.recent_files.insert(0, pstr);
                self.cfg.recent_files.truncate(10);
                save_config(&self.cfg);

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
// 菜单同步 & 右键上下文菜单
// ============================================================================

fn sync_recent_menu(state: &AppState) {
    if state.recent_menu.is_null() {
        return;
    }
    unsafe {
        while DeleteMenu(state.recent_menu, 0, MF_BYPOSITION) != 0 {}
        if state.cfg.recent_files.is_empty() {
            let empty_w = to_wide("（无历史记录）");
            AppendMenuW(state.recent_menu, MF_STRING | MF_GRAYED, 0, empty_w.as_ptr());
        } else {
            for (idx, p) in state.cfg.recent_files.iter().take(10).enumerate() {
                let label = format!("&{}  {}", idx + 1, p);
                let wl = to_wide(&label);
                AppendMenuW(state.recent_menu, MF_STRING, IDM_FILE_RECENT_BASE + idx, wl.as_ptr());
            }
            AppendMenuW(state.recent_menu, MF_SEPARATOR, 0, null());
            let clr_w = to_wide("清空历史记录(&C)");
            AppendMenuW(state.recent_menu, MF_STRING, IDM_FILE_RECENT_CLEAR, clr_w.as_ptr());
        }
    }
}

fn sync_menu_checks(state: &AppState, hmenu: HMENU) {
    let chk = |id: usize, on: bool| unsafe {
        CheckMenuItem(hmenu, id as u32, MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED });
    };
    chk(IDM_VIEW_WRAP, state.cfg.word_wrap);
    chk(IDM_VIEW_LINENUM, state.cfg.show_line_numbers);
    chk(IDM_VIEW_WHITESPACE, state.cfg.show_whitespace);
    chk(IDM_VIEW_OUTLINE, state.cfg.outline_visible);
    chk(IDM_VIEW_OUTLINE_FLOAT, state.cfg.outline_floating);
    chk(IDM_VIEW_THEME_DARK, state.cfg.dark_theme);
    chk(IDM_VIEW_THEME_LIGHT, !state.cfg.dark_theme);
    chk(IDM_VIEW_TOPMOST, state.cfg.always_on_top);
    chk(IDM_EOL_CRLF, state.doc.line_ending == LineEnding::Crlf);
    chk(IDM_EOL_LF, state.doc.line_ending == LineEnding::Lf);
    sync_recent_menu(state);
}

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
        add(IDM_VIEW_FOLD_TOGGLE, "折叠 / 展开当前标题\tCtrl+M", true, false);
        add(IDM_VIEW_FOLD_ALL, "全部折叠正文标题", true, false);
        add(IDM_VIEW_UNFOLD_ALL, "全部展开正文标题", true, false);
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

fn show_outline_context_menu(state: &AppState, owner_hwnd: HWND, mx: i32, my: i32) {
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
        add(
            IDM_VIEW_OUTLINE_FLOAT,
            if state.cfg.outline_floating { "停靠到正文右侧(&D)" } else { "分离为浮动窗口(&F)" },
        );
        add(IDM_VIEW_OUTLINE, "关闭大纲栏(&X)");

        let mut pt = POINT { x: mx, y: my };
        ClientToScreen(owner_hwnd, &mut pt);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, state.hwnd, null());
        DestroyMenu(menu);
    }
}

// ============================================================================
// 大纲交互、浮动大纲窗口过程与指令分发
// ============================================================================

fn handle_outline_click(state: &mut AppState, x: i32, y: i32, left: i32, right: i32) {
    if y < OUTLINE_HEADER_H {
        if y >= 4 && y <= OUTLINE_HEADER_H - 4 {
            if x >= right - 134 && x <= right - 95 {
                state.outline.refresh_from_doc(&state.doc);
                state.invalidate_outline();
            } else if x >= right - 92 && x <= right - 72 {
                state.outline.set_all_collapsed(false);
                state.invalidate_outline();
            } else if x >= right - 69 && x <= right - 49 {
                state.outline.set_all_collapsed(true);
                state.invalidate_outline();
            } else if x >= right - 46 && x <= right - 25 {
                handle_command(state, IDM_VIEW_OUTLINE_FLOAT);
            } else if x >= right - 22 && x <= right - 4 {
                handle_command(state, IDM_VIEW_OUTLINE);
            }
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
        state.invalidate_outline();
        return;
    }

    state.outline.selected_idx = Some(node_idx);
    if state.doc.ensure_line_unfolded(node.line) {
        state.rebuild_fold_row_counts();
    }
    state.doc.cursors = vec![Cursor::collapsed(node.byte_pos)];
    let target_vrow = state.byte_to_visual_row(node.byte_pos);
    let max_y = state.total_visual_lines().saturating_sub(1);
    state.scroll_y = target_vrow.saturating_sub(2).min(max_y);
    state.update_scrollbars();
    unsafe {
        SetFocus(state.hwnd);
    }
    state.invalidate();
    state.invalidate_outline();
}

unsafe extern "system" fn outline_wnd_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let st = &mut *ptr;

    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_floating_outline(st, hdc, rc.right - rc.left, rc.bottom - rc.top);
            EndPaint(hwnd, &ps);
            0
        }
        WM_NCHITTEST => {
            let mut pt = POINT { x: get_x_lparam(lp), y: get_y_lparam(lp) };
            ScreenToClient(hwnd, &mut pt);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            let b = 5;
            let on_l = pt.x < b;
            let on_r = pt.x >= rc.right - b;
            let on_t = pt.y < b;
            let on_bot = pt.y >= rc.bottom - b;
            if on_t && on_l { return HTTOPLEFT as isize; }
            if on_t && on_r { return HTTOPRIGHT as isize; }
            if on_bot && on_l { return HTBOTTOMLEFT as isize; }
            if on_bot && on_r { return HTBOTTOMRIGHT as isize; }
            if on_l { return HTLEFT as isize; }
            if on_r { return HTRIGHT as isize; }
            if on_t { return HTTOP as isize; }
            if on_bot { return HTBOTTOM as isize; }
            if pt.y < OUTLINE_HEADER_H && pt.x < rc.right - 138 {
                return HTCAPTION as isize;
            }
            HTCLIENT as isize
        }
        WM_LBUTTONDOWN => {
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            handle_outline_click(st, get_x_lparam(lp), get_y_lparam(lp), 0, rc.right);
            0
        }
        WM_RBUTTONUP => {
            show_outline_context_menu(st, hwnd, get_x_lparam(lp), get_y_lparam(lp));
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp >> 16) as i16) as i32;
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            let vis_len = st.outline.visible_indices().len();
            let page_rows = ((rc.bottom - OUTLINE_HEADER_H) / OUTLINE_ROW_H).max(1) as usize;
            let max_row = vis_len.saturating_sub(page_rows);
            let step = if delta > 0 { -2isize } else { 2isize };
            st.outline.scroll_row = (st.outline.scroll_row as isize + step).clamp(0, max_row as isize) as usize;
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_EXITSIZEMOVE => {
            let mut rc: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rc);
            st.cfg.outline_x = rc.left;
            st.cfg.outline_y = rc.top;
            st.cfg.outline_float_w = (rc.right - rc.left).max(200);
            st.cfg.outline_float_h = (rc.bottom - rc.top).max(220);
            save_config(&st.cfg);
            0
        }
        WM_SIZE => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_CLOSE => {
            st.cfg.outline_visible = false;
            save_config(&st.cfg);
            st.sync_outline_window();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
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
    state.invalidate_outline();
    state.invalidate();
}

pub fn handle_command(state: &mut AppState, id: usize) {
    match id {
        IDM_FILE_NEW => {
            if state.confirm_discard() {
                state.doc.set_content(String::new(), None, DocEncoding::Utf8, LineEnding::Crlf, 0);
                state.scroll_y = 0;
                state.scroll_x = 0;
                state.update_gutter_width();
                state.reset_wraps_lazy();
                state.update_scrollbars();
                if state.cfg.outline_visible {
                    state.outline.refresh_from_doc(&state.doc);
                    state.invalidate_outline();
                } else {
                    state.outline.nodes.clear();
                }
                if state.search.visible {
                    state.refresh_search_ui();
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
                state.invalidate_outline();
            }
        }
        IDM_FILE_RECENT_BASE..=1109 => {
            let idx = id - IDM_FILE_RECENT_BASE;
            if let Some(pstr) = state.cfg.recent_files.get(idx).cloned() {
                if state.confirm_discard() {
                    let p = PathBuf::from(&pstr);
                    if p.exists() {
                        state.open_file_path(&p);
                    } else {
                        state.cfg.recent_files.remove(idx);
                        save_config(&state.cfg);
                        show_fatal_box("文件不存在", &format!("找不到文件，已从历史记录中移除:\n{}", pstr));
                    }
                }
            }
        }
        IDM_FILE_RECENT_CLEAR => {
            state.cfg.recent_files.clear();
            save_config(&state.cfg);
        }
        IDM_FILE_EXIT => unsafe {
            PostMessageW(state.hwnd, WM_CLOSE, 0, 0);
        },

        IDM_EDIT_UNDO => {
            if state.doc.undo() {
                state.after_doc_edited();
                state.invalidate();
            }
        }
        IDM_EDIT_REDO => {
            if state.doc.redo() {
                state.after_doc_edited();
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
            state.after_doc_edited();
            state.invalidate();
        }
        IDM_EDIT_PASTE => {
            if let Some(txt) = clipboard_paste(state.hwnd) {
                state.doc.insert_text(&txt);
                state.after_doc_edited();
                state.invalidate();
            }
        }
        IDM_EDIT_DELETE => {
            state.doc.delete_forward();
            state.after_doc_edited();
            state.invalidate();
        }
        IDM_EDIT_SELECT_ALL => {
            state.doc.cursors = vec![Cursor { anchor: 0, head: state.doc.text.len() }];
            state.invalidate();
        }
        IDM_EDIT_DUP_LINE => {
            state.doc.duplicate_current_line();
            state.after_doc_edited();
            state.invalidate();
        }
        IDM_EDIT_DEL_LINE => {
            state.doc.delete_current_line();
            state.after_doc_edited();
            state.invalidate();
        }
        IDM_EDIT_MOVE_UP => {
            state.doc.move_lines(true);
            state.after_doc_edited();
            state.invalidate();
        }
        IDM_EDIT_MOVE_DOWN => {
            state.doc.move_lines(false);
            state.after_doc_edited();
            state.invalidate();
        }
        IDM_EDIT_COMMENT => {
            state.doc.toggle_comment(state.doc.lang);
            state.after_doc_edited();
            state.invalidate();
        }
        IDM_EDIT_UPPER | IDM_EDIT_LOWER => {
            state.doc.transform_case(id == IDM_EDIT_UPPER);
            state.after_doc_edited();
            state.invalidate();
        }

        IDM_SEARCH_FIND => state.show_find_panel(false),
        IDM_SEARCH_REPLACE => state.show_find_panel(true),
        IDM_SEARCH_NEXT | IDM_SEARCH_PREV => {
            if !state.search.query.is_empty() {
                if state.search.matches.is_empty() {
                    state.refresh_search_ui();
                }
                let sel = state.doc.primary().range();
                if let Some((ms, me)) = state.search.step_from_caret(id == IDM_SEARCH_NEXT, sel) {
                    state.jump_to_search_match(ms, me);
                }
            } else {
                state.show_find_panel(false);
            }
        }
        IDM_SEARCH_GOTO => show_goto_dialog(state),

        IDM_VIEW_WRAP => {
            state.cfg.word_wrap = !state.cfg.word_wrap;
            state.reset_wraps_lazy();
            state.ensure_caret_visible();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_LINENUM => {
            state.cfg.show_line_numbers = !state.cfg.show_line_numbers;
            state.update_gutter_width();
            state.reset_wraps_lazy();
            state.update_scrollbars();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_WHITESPACE => {
            state.cfg.show_whitespace = !state.cfg.show_whitespace;
            state.reset_wraps_lazy();
            save_config(&state.cfg);
            state.invalidate();
        }
        IDM_VIEW_FOLD_TOGGLE => {
            let cur_line = state.doc.line_of_byte(state.doc.primary().head);
            if let Some(hl) = state.doc.heading_for_line(cur_line) {
                if state.doc.toggle_fold_at_line(hl) {
                    let pos = state.doc.line_bounds(hl).0;
                    state.doc.cursors = vec![Cursor::collapsed(pos)];
                    state.rebuild_fold_row_counts();
                    state.ensure_caret_visible();
                    state.invalidate();
                }
            }
        }
        IDM_VIEW_FOLD_ALL => {
            state.doc.set_all_folds(true);
            let cur_line = state.doc.line_of_byte(state.doc.primary().head);
            if state.doc.line_hidden.get(cur_line).copied().unwrap_or(false) {
                if let Some(hl) = state.doc.heading_for_line(cur_line) {
                    state.doc.cursors = vec![Cursor::collapsed(state.doc.line_bounds(hl).0)];
                } else {
                    state.doc.cursors = vec![Cursor::collapsed(0)];
                }
            }
            state.rebuild_fold_row_counts();
            state.ensure_caret_visible();
            state.invalidate();
        }
        IDM_VIEW_UNFOLD_ALL => {
            state.doc.set_all_folds(false);
            state.rebuild_fold_row_counts();
            state.ensure_caret_visible();
            state.invalidate();
        }
        IDM_VIEW_OUTLINE => {
            state.cfg.outline_visible = !state.cfg.outline_visible;
            if state.cfg.outline_visible {
                state.outline.refresh_from_doc(&state.doc);
            }
            save_config(&state.cfg);
            state.sync_outline_window();
        }
        IDM_VIEW_OUTLINE_FLOAT => {
            state.cfg.outline_floating = !state.cfg.outline_floating;
            if !state.cfg.outline_visible {
                state.cfg.outline_visible = true;
                state.outline.refresh_from_doc(&state.doc);
            }
            save_config(&state.cfg);
            state.sync_outline_window();
        }
        IDM_VIEW_OUTLINE_REFRESH => {
            if !state.cfg.outline_visible {
                state.cfg.outline_visible = true;
                save_config(&state.cfg);
                state.sync_outline_window();
            }
            state.outline.refresh_from_doc(&state.doc);
            state.invalidate_outline();
        }
        IDM_OUTLINE_EXPAND_ALL => {
            state.outline.set_all_collapsed(false);
            state.invalidate_outline();
        }
        IDM_OUTLINE_COLLAPSE_ALL => {
            state.outline.set_all_collapsed(true);
            state.invalidate_outline();
        }
        IDM_VIEW_FONT => dialog_choose_font(state),
        IDM_VIEW_THEME_DARK => apply_theme_choice(state, true),
        IDM_VIEW_THEME_LIGHT => apply_theme_choice(state, false),
        IDM_VIEW_TOPMOST => {
            state.cfg.always_on_top = !state.cfg.always_on_top;
            let insert_after = if state.cfg.always_on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
            unsafe {
                SetWindowPos(state.hwnd, insert_after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
                if !state.outline_hwnd.is_null() {
                    SetWindowPos(state.outline_hwnd, insert_after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
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
            0x4D => { handle_command(state, IDM_VIEW_FOLD_TOGGLE); return true; }
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
    let (line, _) = state.doc.byte_to_line_col(head);
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
            let vrow = state.byte_to_visual_row(head);
            let x = state.measure_byte_x(head);
            let (pos, _, _) = state.hit_test_vrow_x(vrow.saturating_sub(1), x);
            move_cursor(state, pos);
            true
        }
        VK_DOWN => {
            let vrow = state.byte_to_visual_row(head);
            let x = state.measure_byte_x(head);
            let target_vrow = (vrow + 1).min(state.total_visual_lines().saturating_sub(1));
            let (pos, _, _) = state.hit_test_vrow_x(target_vrow, x);
            move_cursor(state, pos);
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
            let vrow = state.byte_to_visual_row(head).saturating_sub(step);
            let x = state.measure_byte_x(head);
            let (pos, _, _) = state.hit_test_vrow_x(vrow, x);
            move_cursor(state, pos);
            true
        }
        VK_NEXT => {
            let step = state.visible_row_count();
            let vrow = (state.byte_to_visual_row(head) + step).min(state.total_visual_lines().saturating_sub(1));
            let x = state.measure_byte_x(head);
            let (pos, _, _) = state.hit_test_vrow_x(vrow, x);
            move_cursor(state, pos);
            true
        }
        VK_BACK => {
            state.doc.backspace();
            state.after_doc_edited();
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
            let nh = ((lp >> 16) & 0xFFFF) as i16 as i32;
            if nw > 60 && nh > 60 {
                st.client_w = nw;
                st.client_h = nh;
                st.reset_wraps_lazy();
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
            // 来自子控件（如查找面板按钮/标签）的光标事件直接交还 DefWindowProcW
            if wp as HWND != hwnd {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            if loword(lp as usize) as u32 == HTCLIENT as u32 {
                let mut pt: POINT = std::mem::zeroed();
                GetCursorPos(&mut pt);
                ScreenToClient(hwnd, &mut pt);
                let edit_r = st.editor_right();
                let docked_outline = st.cfg.outline_visible && !st.cfg.outline_floating;
                if docked_outline && pt.y < st.client_h - STATUS_BAR_HEIGHT && (pt.x - edit_r).abs() <= 4 {
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
            st.after_doc_edited();
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
            let docked_outline = st.cfg.outline_visible && !st.cfg.outline_floating;
            if docked_outline && my < st.client_h - STATUS_BAR_HEIGHT && (mx - edit_r).abs() <= 4 {
                SetCapture(hwnd);
                st.resizing_outline = true;
                return 0;
            }
            if docked_outline && mx >= edit_r && my < st.client_h - STATUS_BAR_HEIGHT {
                handle_outline_click(st, mx, my, edit_r, st.client_w);
                return 0;
            }
            if mx >= st.gutter_w - FOLD_COL_W && mx < st.gutter_w && my < st.client_h - STATUS_BAR_HEIGHT {
                let vrow = st.scroll_y + (my.max(0) / st.line_h.max(1)) as usize;
                if let Some(seg) = st.get_visual_seg(vrow) {
                    if seg.b_start == 0 && st.doc.line_levels.get(seg.line).copied().unwrap_or(0) > 0 {
                        st.doc.toggle_fold_at_line(seg.line);
                        st.rebuild_fold_row_counts();
                        st.invalidate();
                        return 0;
                    }
                }
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
                    st.reset_wraps_lazy();
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
            if st.cfg.outline_visible && !st.cfg.outline_floating && mx >= st.editor_right() {
                show_outline_context_menu(st, hwnd, mx, my);
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
            if st.cfg.outline_visible && !st.cfg.outline_floating && pt.x >= st.editor_right() && pt.y < st.client_h - STATUS_BAR_HEIGHT {
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
            if !st.outline_hwnd.is_null() {
                let mut orc: RECT = std::mem::zeroed();
                GetWindowRect(st.outline_hwnd, &mut orc);
                st.cfg.outline_x = orc.left;
                st.cfg.outline_y = orc.top;
                st.cfg.outline_float_w = (orc.right - orc.left).max(200);
                st.cfg.outline_float_h = (orc.bottom - orc.top).max(220);
                DestroyWindow(st.outline_hwnd);
                st.outline_hwnd = null_mut();
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
