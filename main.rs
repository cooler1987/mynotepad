#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod ui;

use std::path::PathBuf;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Shell::DragAcceptFiles;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use engine::{
    init_logger, load_config, log_msg, show_fatal_box, to_wide, Document, OutlineState, SearchState,
};
use ui::{create_find_panel, main_wnd_proc, AppState};

// 菜单命令 ID（与 ui.rs 共享）
pub const IDM_FILE_NEW: usize = 1001;
pub const IDM_FILE_OPEN: usize = 1002;
pub const IDM_FILE_SAVE: usize = 1003;
pub const IDM_FILE_SAVEAS: usize = 1004;
pub const IDM_FILE_RELOAD: usize = 1005;
pub const IDM_FILE_EXIT: usize = 1006;

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
pub const IDM_VIEW_THEME: usize = 4007;
pub const IDM_VIEW_TOPMOST: usize = 4008;
pub const IDM_VIEW_ZOOM_IN: usize = 4009;
pub const IDM_VIEW_ZOOM_OUT: usize = 4010;
pub const IDM_VIEW_ZOOM_RESET: usize = 4011;

pub const IDM_EOL_CRLF: usize = 5201;
pub const IDM_EOL_LF: usize = 5202;

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
        add(m_view, IDM_VIEW_OUTLINE, "Markdown 大纲栏(&O)\tCtrl+Shift+O");
        add(m_view, IDM_VIEW_OUTLINE_REFRESH, "刷新大纲目录(&E)");
        AppendMenuW(m_view, MF_SEPARATOR, 0, null());
        add(m_view, IDM_VIEW_FONT, "字体与字号设置(&F)...");
        add(m_view, IDM_VIEW_THEME, "切换深色 / 浅色主题(&T)");
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

        root
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

        let mut state = Box::new(AppState {
            hwnd,
            hfont_regular: null_mut(),
            hfont_bold: null_mut(),
            hfont_italic: null_mut(),
            hfont_strike: null_mut(),
            hfont_ui,
            hfont_ui_bold,
            doc: Document::new_empty(),
            cfg,
            search: SearchState::new(),
            outline: OutlineState::new(),
            client_w: 1080,
            client_h: 720,
            line_h: 24,
            avg_char_w: 9,
            gutter_w: 14,
            scroll_y: 0,
            scroll_x: 0,
            line_wraps: Vec::new(),
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

        state.recreate_fonts();
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
