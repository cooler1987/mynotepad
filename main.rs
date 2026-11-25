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

use engine::{init_logger, load_config, log_msg, show_fatal_box, to_wide, Document, Lang, SearchState};
use ui::{build_main_menu, create_find_panel, main_wnd_proc, AppState};

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
