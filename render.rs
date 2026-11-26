use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::engine::*;
use crate::ui::AppState;

pub const STATUS_BAR_HEIGHT: i32 = 24;
pub const OUTLINE_HEADER_H: i32 = 30;
pub const OUTLINE_ROW_H: i32 = 24;

// ============================================================================
// 1. 深色与浅色两套完整主题配色 + 系统标题栏深色适配
// ============================================================================

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: u32,
    pub gutter_bg: u32,
    pub gutter_fg: u32,
    pub cur_line_bg: u32,
    pub code_block_bg: u32,
    pub inline_code_bg: u32,
    pub sel_bg: u32,
    pub match_bg: u32,
    pub cur_match_bg: u32,
    pub bracket_bg: u32,
    pub text_fg: u32,
    pub caret: u32,
    pub ws_fg: u32,
    pub status_bg: u32,
    pub status_fg: u32,
    pub status_border: u32,
    pub outline_bg: u32,
    pub outline_hdr_bg: u32,
    pub outline_sel_bg: u32,
    pub outline_btn_bg: u32,
    pub outline_fg: u32,
    pub outline_muted: u32,
    pub kw_fg: u32,
    pub h1_fg: u32,
    pub h2_fg: u32,
    pub str_fg: u32,
    pub cmt_fg: u32,
    pub num_fg: u32,
    pub acc_fg: u32,
    pub bold_fg: u32,
    pub italic_fg: u32,
    pub strike_fg: u32,
    pub code_fg: u32,
    pub alert_fg: u32,
    pub task_done_fg: u32,
    pub task_todo_fg: u32,
}

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

pub fn get_theme(dark: bool) -> Theme {
    if dark {
        Theme {
            bg: rgb(30, 30, 30),
            gutter_bg: rgb(35, 35, 37),
            gutter_fg: rgb(125, 125, 125),
            cur_line_bg: rgb(40, 43, 47),
            code_block_bg: rgb(46, 42, 28),
            inline_code_bg: rgb(58, 52, 34),
            sel_bg: rgb(38, 79, 120),
            match_bg: rgb(90, 70, 30),
            cur_match_bg: rgb(160, 110, 30),
            bracket_bg: rgb(65, 95, 65),
            text_fg: rgb(216, 216, 216),
            caret: rgb(240, 240, 240),
            ws_fg: rgb(85, 85, 85),
            status_bg: rgb(0, 122, 204),
            status_fg: rgb(255, 255, 255),
            status_border: rgb(55, 55, 60),
            outline_bg: rgb(36, 37, 40),
            outline_hdr_bg: rgb(45, 46, 50),
            outline_sel_bg: rgb(55, 76, 102),
            outline_btn_bg: rgb(62, 65, 72),
            outline_fg: rgb(224, 224, 224),
            outline_muted: rgb(148, 154, 164),
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
            code_block_bg: rgb(253, 249, 224),
            inline_code_bg: rgb(250, 242, 202),
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
            outline_btn_bg: rgb(224, 228, 235),
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

pub fn apply_titlebar_theme(hwnd: HWND, dark: bool) {
    type DwmSetAttrFn = unsafe extern "system" fn(HWND, u32, *const i32, u32) -> i32;
    static DWM_FN: OnceLock<Option<usize>> = OnceLock::new();
    let addr = *DWM_FN.get_or_init(|| unsafe {
        let dll = to_wide("dwmapi.dll");
        let hmod = LoadLibraryW(dll.as_ptr());
        if hmod.is_null() {
            return None;
        }
        GetProcAddress(hmod, b"DwmSetWindowAttribute\0".as_ptr()).map(|f| f as usize)
    });
    if let Some(ptr) = addr {
        unsafe {
            let func: DwmSetAttrFn = std::mem::transmute(ptr);
            let val: i32 = if dark { 1 } else { 0 };
            let _ = func(hwnd, 20, &val, std::mem::size_of::<i32>() as u32);
            let _ = func(hwnd, 19, &val, std::mem::size_of::<i32>() as u32);
        }
    }
}

// ============================================================================
// 2. Tab 制表位展开与像素级自动换行引擎
// ============================================================================

pub fn expand_tabs_col(s: &str, start_col: usize, show_ws: bool) -> (String, usize) {
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

pub fn expand_tabs_with_u16_map(s: &str, show_ws: bool, half_snap: bool) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len() + 8);
    let mut map = Vec::with_capacity(s.len() + 4);
    let mut col = 0usize;
    for (b_idx, ch) in s.char_indices() {
        if ch == '\t' {
            let spaces = 4 - (col % 4);
            for k in 0..spaces {
                out.push(if show_ws && k == 0 { '→' } else { ' ' });
                if half_snap && k >= spaces / 2 {
                    map.push(b_idx + 1);
                } else {
                    map.push(b_idx);
                }
            }
            col += spaces;
        } else if ch == ' ' && show_ws {
            out.push('·');
            map.push(b_idx);
            col += 1;
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

pub fn compute_line_wraps_with_dc(
    hdc: HDC,
    line_str: &str,
    max_px: i32,
    avg_char_w: i32,
    show_ws: bool,
    font_regular: HFONT,
    font_bold: HFONT,
    is_md: bool,
) -> Vec<(usize, usize)> {
    if line_str.is_empty() {
        return vec![(0, 0)];
    }

    // 仅对超过 2MB 的极端单行走快速列切分，避免单行卡顿
    if line_str.len() > 2 * 1024 * 1024 {
        let cols = (max_px / avg_char_w.max(6)).max(12) as usize;
        let mut segs = Vec::new();
        let mut s = 0usize;
        while s < line_str.len() {
            let e = line_str[s..]
                .char_indices()
                .nth(cols)
                .map(|(i, _)| s + i)
                .unwrap_or(line_str.len());
            segs.push((s, e));
            s = e;
        }
        return segs;
    }

    let use_bold = is_md
        && (line_str.trim_start().starts_with('#') || line_str.contains("**") || line_str.contains("__"));
    unsafe {
        SelectObject(hdc, if use_bold { font_bold } else { font_regular } as _);
    }

    let mut segs = Vec::new();
    let mut seg_start = 0usize;
    let base_chars = ((max_px / (avg_char_w * 3 / 4).max(4)) as usize + 32).clamp(64, 240);

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
            let (exp, map) = expand_tabs_with_u16_map(ch_slice, show_ws, false);
            let w = to_wide(&exp);
            let u16_len = (w.len() - 1) as i32;
            let mut fit: i32 = 0;
            let mut sz: SIZE = unsafe { std::mem::zeroed() };
            unsafe {
                GetTextExtentExPointW(hdc, w.as_ptr(), u16_len, max_px, &mut fit, null_mut(), &mut sz);
            }
            if fit == u16_len && chunk_end < rem.len() {
                chunk_chars = (chunk_chars * 2).min(4096);
                if chunk_chars == 4096 && chunk_end < rem.len() {
                    // 极窄字符超长行继续扩大
                    chunk_chars = rem.len();
                }
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

// ============================================================================
// 3. 括号匹配与双缓冲 GDI 渲染器（编辑区 + 大纲栏 + 状态栏）
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

pub fn paint_editor(state: &mut AppState, hdc_win: HDC) {
    // 仅在绘制前对视口可见行做像素级精确折行（惰性按需缓存）
    state.ensure_viewport_wraps();

    let w = state.client_w.max(1);
    let h = state.client_h.max(1);
    let edit_r = state.editor_right();
    let edit_b = (h - STATUS_BAR_HEIGHT).max(1);
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
        let mut cached_hl_line = usize::MAX;
        let mut cached_tokens: Vec<(usize, usize, TokenKind)> = Vec::new();
        let mut cached_attr = LineAttr::default();

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

            let full_line = state.doc.line_str(seg.line);
            if seg.line != cached_hl_line {
                cached_hl_line = seg.line;
                if full_line.len() <= 8192 {
                    let line_state = state.doc.line_states.get(seg.line).copied().unwrap_or(LineState::Normal);
                    let (t, a) = highlight_line_with_state(active_lang, full_line, line_state);
                    cached_tokens = t;
                    cached_attr = a;
                } else {
                    cached_tokens.clear();
                    cached_attr = LineAttr::default();
                }
            }

            if cached_attr.is_code_block {
                let cb_rc = RECT { left: state.gutter_w, top: y, right: edit_r, bottom: y + state.line_h };
                FillRect(mem_dc, &cb_rc, code_blk_br);
            }
            if seg.line == cur_line {
                let row_rc = RECT { left: state.gutter_w, top: y, right: edit_r, bottom: y + state.line_h };
                FillRect(mem_dc, &row_rc, cur_line_br);
            }
            if cached_attr.is_quote {
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

            if cached_attr.is_hr && seg.b_start == 0 {
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
                let (exp, _) = expand_tabs_col(sub, 0, state.cfg.show_whitespace);
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
            for &(ts, te, kind) in &cached_tokens {
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
                    TokenKind::InlineCode => (theme.code_fg, state.hfont_regular, !cached_attr.is_code_block),
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
        let theme_tag = if state.cfg.dark_theme { "深色" } else { "浅色" };
        let right_info = format!(
            "字数 {}   |   {}   |   {}   |   {}   |   {}   |   {}%  ",
            state.doc.total_chars,
            mode_tag,
            theme_tag,
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
