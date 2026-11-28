#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod render;
mod ui;

use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::{BST_CHECKED, EM_SETSEL};
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_ESCAPE, VK_RETURN, VK_SHIFT, VK_TAB};
use windows_sys::Win32::UI::Shell::DragAcceptFiles;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use engine::{
    from_wide_slice, init_logger, load_config, log_msg, normalize_newlines_fast, save_config,
    show_fatal_box, to_wide, Cursor, Document, OutlineState, SearchState,
};
use render::{apply_titlebar_theme, get_theme};
use ui::{main_wnd_proc, AppState};

// 菜单命令 ID
pub const IDM_FILE_NEW: usize = 1001;
pub const IDM_FILE_OPEN: usize = 1002;
pub const IDM_FILE_SAVE: usize = 1003;
pub const IDM_FILE_SAVEAS: usize = 1004;
pub const IDM_FILE_RELOAD: usize = 1005;
pub const IDM_FILE_EXIT: usize = 1006;
pub const IDM_FILE_RECENT_BASE: usize = 1100;
pub const IDM_FILE_RECENT_CLEAR: usize = 1120;

pub const IDM_EDIT_UNDO: usize = 2001;
pub const IDM_EDIT_REDO: usize = 2002;
pub const IDM_EDIT_CUT: usize = 2003;
pub const IDM_EDIT_COPY: usize = 2004;
pub const IDM_EDIT_PASTE: usize = 2005;
pub const IDM_EDIT_SELECT_ALL: usize = 2006;
pub const IDM_EDIT_DUP_LINE: usize = 2007;
pub const IDM_EDIT_DEL_LINE: usize = 2008;
pub const IDM_EDIT_MOVE_UP: usize = 2009;
pub const IDM_EDIT_MOVE_DOWN: usize = 2010;
pub const IDM_EDIT_COMMENT: usize = 2011;
pub const IDM_EDIT_UPPER: usize = 2012;
pub const IDM_EDIT_LOWER: usize = 2013;
pub const IDM_EDIT_DELETE: usize = 2014;

pub const IDM_SEARCH_FIND: usize = 3001;
pub const IDM_SEARCH_REPLACE: usize = 3002;
pub const IDM_SEARCH_GOTO: usize = 3003;
pub const IDM_SEARCH_NEXT: usize = 3004;
pub const IDM_SEARCH_PREV: usize = 3005;

pub const IDM_VIEW_WRAP: usize = 4001;
pub const IDM_VIEW_LINENUM: usize = 4002;
pub const IDM_VIEW_WHITESPACE: usize = 4003;
pub const IDM_VIEW_OUTLINE: usize = 4004;
pub const IDM_VIEW_OUTLINE_REFRESH: usize = 4005;
pub const IDM_VIEW_FONT: usize = 4006;
pub const IDM_VIEW_THEME_DARK: usize = 4007;
pub const IDM_VIEW_THEME_LIGHT: usize = 4008;
pub const IDM_VIEW_TOPMOST: usize = 4009;
pub const IDM_VIEW_ZOOM_IN: usize = 4010;
pub const IDM_VIEW_ZOOM_OUT: usize = 4011;
pub const IDM_VIEW_ZOOM_RESET: usize = 4012;
pub const IDM_OUTLINE_EXPAND_ALL: usize = 4020;
pub const IDM_OUTLINE_COLLAPSE_ALL: usize = 4021;
pub const IDM_VIEW_OUTLINE_FLOAT: usize = 4022;
pub const IDM_VIEW_FOLD_TOGGLE: usize = 4023;
pub const IDM_VIEW_FOLD_ALL: usize = 4024;
pub const IDM_VIEW_UNFOLD_ALL: usize = 4025;

pub const IDM_EOL_CRLF: usize = 5201;
pub const IDM_EOL_LF: usize = 5202;

// Markdown 段落转换命令 ID
pub const IDM_MD_BLK_PARA: usize = 7001;
pub const IDM_MD_BLK_H1: usize = 7002;
pub const IDM_MD_BLK_H2: usize = 7003;
pub const IDM_MD_BLK_H3: usize = 7004;
pub const IDM_MD_BLK_H4: usize = 7005;
pub const IDM_MD_BLK_H5: usize = 7006;
pub const IDM_MD_BLK_BULLET: usize = 7007;
pub const IDM_MD_BLK_ORDERED: usize = 7008;
pub const IDM_MD_BLK_TASK: usize = 7009;
pub const IDM_MD_BLK_QUOTE: usize = 7010;
pub const IDM_MD_BLK_CODE: usize = 7011;
pub const IDM_MD_INDENT: usize = 7012;
pub const IDM_MD_OUTDENT: usize = 7013;

// Markdown 行内格式命令 ID
pub const IDM_MD_FMT_BOLD: usize = 7101;
pub const IDM_MD_FMT_ITALIC: usize = 7102;
pub const IDM_MD_FMT_CODE: usize = 7103;
pub const IDM_MD_FMT_STRIKE: usize = 7104;
pub const IDM_MD_FMT_LINK: usize = 7105;

// Markdown 插入模板命令 ID
pub const IDM_MD_INS_H1: usize = 7201;
pub const IDM_MD_INS_H2: usize = 7202;
pub const IDM_MD_INS_H3: usize = 7203;
pub const IDM_MD_INS_H4: usize = 7204;
pub const IDM_MD_INS_H5: usize = 7205;
pub const IDM_MD_INS_BULLET: usize = 7206;
pub const IDM_MD_INS_ORDERED: usize = 7207;
pub const IDM_MD_INS_TASK: usize = 7208;
pub const IDM_MD_INS_QUOTE: usize = 7209;
pub const IDM_MD_INS_CODE: usize = 7210;
pub const IDM_MD_INS_HR: usize = 7211;
pub const IDM_MD_INS_TABLE: usize = 7212;
pub const IDM_MD_INS_NOTE: usize = 7213;
pub const IDM_MD_INS_TIP: usize = 7214;
pub const IDM_MD_INS_IMPORTANT: usize = 7215;
pub const IDM_MD_INS_WARNING: usize = 7216;
pub const IDM_MD_INS_CAUTION: usize = 7217;
pub const IDM_MD_INS_LINK: usize = 7218;
pub const IDM_MD_INS_FORMULA: usize = 7219;
pub const IDM_MD_INS_IMAGE: usize = 7220;

// 代码块专项操作命令 ID
pub const IDM_MD_CB_COPY_INNER: usize = 7301;
pub const IDM_MD_CB_COPY_FULL: usize = 7302;
pub const IDM_MD_CB_DELETE: usize = 7303;
pub const IDM_MD_CB_FOLD_TOGGLE: usize = 7304;

pub const FIND_PANEL_W: i32 = 430;
pub const FIND_PANEL_H: i32 = 108;
pub const CF_UNICODETEXT_ID: u32 = 13;
const SS_CENTER: u32 = 1;

const IDC_FIND_EDIT: i32 = 6001;
const IDC_REPLACE_EDIT: i32 = 6002;
const IDC_CHK_CASE: i32 = 6003;
const IDC_CHK_WORD: i32 = 6004;
const IDC_CHK_REGEX: i32 = 6005;
const IDC_BTN_PREV: i32 = 6006;
const IDC_BTN_NEXT: i32 = 6007;
pub const IDC_BTN_REPLACE: i32 = 6008;
pub const IDC_BTN_REPLACE_ALL: i32 = 6009;
const IDC_BTN_CLOSE: i32 = 6010;
const IDC_LBL_COUNT: i32 = 6011;

const IDC_GOTO_EDIT: i32 = 6101;
const IDC_GOTO_OK: i32 = 6102;
const IDC_GOTO_CANCEL: i32 = 6103;

#[inline]
fn lo_word(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}
#[inline]
fn hi_word(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}
#[inline]
fn is_key_down(vk: u16) -> bool {
    unsafe { (GetKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

// ============================================================================
// 1. 剪贴板与系统文件/字体对话框
// ============================================================================

pub fn clipboard_copy(hwnd: HWND, text: &str) {
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

pub fn clipboard_paste(hwnd: HWND) -> Option<String> {
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

pub fn dialog_open_file(hwnd: HWND) -> Option<PathBuf> {
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

pub fn dialog_save_file(hwnd: HWND, current_path: Option<&Path>) -> Option<PathBuf> {
    let mut buf = vec![0u16; 4096];
    if let Some(p) = current_path.and_then(|cp| cp.file_name()) {
        let w = to_wide(&p.to_string_lossy());
        let n = w.len().min(buf.len());
        buf[..n].copy_from_slice(&w[..n]);
    }
    let is_txt = current_path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("txt"));
    let def_ext = to_wide(if is_txt { "txt" } else { "md" });
    let filter: Vec<u16> = "Markdown 文档 (*.md)\0*.md\0文本文档 (*.txt)\0*.txt\0所有文件 (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    unsafe {
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = hwnd;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.nFilterIndex = if is_txt { 2 } else { 1 };
        ofn.lpstrFile = buf.as_mut_ptr();
        ofn.nMaxFile = buf.len() as u32;
        ofn.lpstrDefExt = def_ext.as_ptr();
        ofn.Flags = OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_HIDEREADONLY;
        if GetSaveFileNameW(&mut ofn) != 0 {
            let mut path = PathBuf::from(from_wide_slice(&buf));
            if path.extension().is_none() {
                match ofn.nFilterIndex {
                    1 => {
                        path.set_extension("md");
                    }
                    2 => {
                        path.set_extension("txt");
                    }
                    _ => {}
                }
            }
            return Some(path);
        }
    }
    None
}

pub fn dialog_choose_font(state: &mut AppState) {
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
// 2. 主题化悬浮查找替换面板 & 转到指定行弹窗
// ============================================================================

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
            let forward = !is_key_down(VK_SHIFT);
            if st.search.matches.is_empty() && !st.search.query.is_empty() {
                st.refresh_search_ui();
            }
            let sel = st.doc.primary().range();
            if let Some((ms, me)) = st.search.step_from_caret(forward, sel) {
                st.jump_to_search_match(ms, me);
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
                let id = lo_word(wp) as i32;
                let code = hi_word(wp) as u32;
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
                        if st.search.matches.is_empty() && !st.search.query.is_empty() {
                            st.refresh_search_ui();
                        }
                        let sel = st.doc.primary().range();
                        if let Some((ms, me)) = st.search.step_from_caret(id == IDC_BTN_NEXT, sel) {
                            st.jump_to_search_match(ms, me);
                        }
                        return 0;
                    }
                    IDC_BTN_REPLACE => {
                        if let Some(idx) = st.search.current_idx {
                            if let Some(&(ms, me)) = st.search.matches.get(idx) {
                                let rep = st.search.expand_single_replace(&st.doc, ms, me);
                                let rlen = rep.len();
                                st.doc.apply_edits(vec![(ms, me, rep)], Some(vec![Cursor::collapsed(ms + rlen)]));
                                st.after_doc_edited();
                                let sel = st.doc.primary().range();
                                if let Some((ns, ne)) = st.search.step_from_caret(true, sel) {
                                    st.jump_to_search_match(ns, ne);
                                }
                            }
                        }
                        return 0;
                    }
                    IDC_BTN_REPLACE_ALL => {
                        let edits = st.search.build_replace_all_edits(&st.doc.text);
                        if !edits.is_empty() {
                            st.doc.apply_edits(edits, None);
                            st.after_doc_edited();
                            st.invalidate();
                        } else {
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
            let id = lo_word(wp) as i32;
            if id == IDC_GOTO_OK {
                let s = get_win_text(st.goto_edit);
                if let Ok(line_num) = s.trim().parse::<usize>() {
                    let target = line_num.saturating_sub(1).min(st.doc.line_count().saturating_sub(1));
                    if st.doc.ensure_line_unfolded(target) {
                        st.reset_wraps_lazy();
                    }
                    let pos = st.doc.line_col_to_byte(target, 0);
                    st.doc.cursors = vec![Cursor::collapsed(pos)];
                    st.center_caret_in_view();
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

pub fn show_goto_dialog(state: &mut AppState) {
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

// ============================================================================
// 3. 主菜单构建与程序入口
// ============================================================================

pub fn build_main_menu() -> (HMENU, HMENU) {
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
        let m_recent = CreatePopupMenu();
        let empty_w = to_wide("（无历史记录）");
        AppendMenuW(m_recent, MF_STRING | MF_GRAYED, 0, empty_w.as_ptr());
        add_pop(m_file, m_recent, "最近打开的文件(&H)");
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
        add(m_edit, IDM_EDIT_DELETE, "删除(&D)\tDel");
        add(m_edit, IDM_EDIT_SELECT_ALL, "全选(&A)\tCtrl+A");
        AppendMenuW(m_edit, MF_SEPARATOR, 0, null());
        add(m_edit, IDM_EDIT_DUP_LINE, "复制当前行\tCtrl+D");
        add(m_edit, IDM_EDIT_DEL_LINE, "删除当前行\tCtrl+Shift+K");
        add(m_edit, IDM_EDIT_MOVE_UP, "上移当前行\tAlt+Up");
        add(m_edit, IDM_EDIT_MOVE_DOWN, "下移当前行\tAlt+Down");
        add(m_edit, IDM_EDIT_COMMENT, "切换引用 / 注释\tCtrl+/");
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
        add(m_view, IDM_VIEW_LINENUM, "显示行号(&L)\tCtrl+Shift+L");
        add(m_view, IDM_VIEW_WHITESPACE, "显示空白字符(&S)");
        AppendMenuW(m_view, MF_SEPARATOR, 0, null());
        add(m_view, IDM_VIEW_FOLD_TOGGLE, "折叠 / 展开当前标题或代码块\tCtrl+M");
        add(m_view, IDM_VIEW_FOLD_ALL, "全部折叠正文标题");
        add(m_view, IDM_VIEW_UNFOLD_ALL, "全部展开正文标题");
        AppendMenuW(m_view, MF_SEPARATOR, 0, null());
        add(m_view, IDM_VIEW_OUTLINE, "Markdown 大纲栏(&O)\tCtrl+Shift+O");
        add(m_view, IDM_VIEW_OUTLINE_FLOAT, "大纲栏浮动窗口(&P)");
        add(m_view, IDM_VIEW_OUTLINE_REFRESH, "刷新大纲目录(&E)");
        AppendMenuW(m_view, MF_SEPARATOR, 0, null());
        add(m_view, IDM_VIEW_THEME_DARK, "深色主题 (Dark)");
        add(m_view, IDM_VIEW_THEME_LIGHT, "浅色主题 (Light)");
        add(m_view, IDM_VIEW_FONT, "字体与字号设置(&F)...");
        add(m_view, IDM_VIEW_TOPMOST, "窗口置顶(&A)");
        AppendMenuW(m_view, MF_SEPARATOR, 0, null());
        add(m_view, IDM_VIEW_ZOOM_IN, "放大字体\tCtrl++");
        add(m_view, IDM_VIEW_ZOOM_OUT, "缩小字体\tCtrl+-");
        add(m_view, IDM_VIEW_ZOOM_RESET, "重置缩放 (100%)\tCtrl+0");
        add_pop(root, m_view, "视图(&V)");

        let m_eol = CreatePopupMenu();
        add(m_eol, IDM_EOL_CRLF, "Windows (CRLF)");
        add(m_eol, IDM_EOL_LF, "Unix / Linux (LF)");
        add_pop(root, m_eol, "换行符(&L)");

        (root, m_recent)
    }
}

fn main() {
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
            let err = format!("RegisterClassExW failed: {}", windows_sys::Win32::Foundation::GetLastError());
            log_msg("FATAL", &err);
            show_fatal_box("初始化窗口类失败", &err);
            return;
        }

        let (hmenu, recent_menu) = build_main_menu();
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
            let err = format!("CreateWindowExW failed: {}", windows_sys::Win32::Foundation::GetLastError());
            log_msg("FATAL", &err);
            show_fatal_box("创建主窗口失败", &err);
            return;
        }

        let ui_font_name = to_wide("Microsoft YaHei UI");
        let hfont_ui = CreateFontW(
            -13,
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
            DEFAULT_PITCH as u32,
            ui_font_name.as_ptr(),
        );
        let hfont_ui_bold = CreateFontW(
            -13,
            0,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            ui_font_name.as_ptr(),
        );

        let dark = cfg.dark_theme;
        let mut state = Box::new(AppState {
            hwnd,
            recent_menu,
            outline_hwnd: null_mut(),
            hfont_regular: null_mut(),
            hfont_bold: null_mut(),
            hfont_italic: null_mut(),
            hfont_strike: null_mut(),
            hfont_ui,
            hfont_ui_bold,
            find_bg_brush: null_mut(),
            find_edit_brush: null_mut(),
            doc: Document::new_empty(),
            cfg,
            search: SearchState::new(),
            outline: OutlineState::new(),
            client_w: 1080,
            client_h: 720,
            line_h: 24,
            avg_char_w: 9,
            gutter_w: 28,
            scroll_y: 0,
            scroll_x: 0,
            wrap_max_px_cached: 0,
            line_wraps: vec![None],
            line_row_counts: vec![1],
            visual_prefix: vec![0, 1],
            dragging: false,
            rect_selecting: false,
            rect_anchor: (0, 0),
            resizing_outline: false,
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

        apply_titlebar_theme(hwnd, dark);
        state.recreate_fonts();
        state.rebuild_theme_brushes();
        create_find_panel(&mut state, hinst);

        let state_ptr: *mut AppState = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        DragAcceptFiles(hwnd, 1);

        if let Some(arg_path) = std::env::args_os().nth(1) {
            let p = PathBuf::from(arg_path);
            if p.exists() {
                (*state_ptr).open_file_path(&p);
            }
        }

        let show_cmd = if (*state_ptr).cfg.maximized { SW_SHOWMAXIMIZED } else { SW_SHOW };
        ShowWindow(hwnd, show_cmd);
        UpdateWindow(hwnd);
        if (*state_ptr).cfg.outline_visible {
            (*state_ptr).outline.refresh_from_doc(&(*state_ptr).doc);
            (*state_ptr).sync_outline_window();
        }
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
