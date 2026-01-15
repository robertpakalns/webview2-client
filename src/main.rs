#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use webview2_com::{
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT, ICoreWebView2Environment,
        ICoreWebView2WebResourceRequestedEventArgs,
    },
    Microsoft::Web::WebView2::Win32::{
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
        ICoreWebView2EnvironmentOptions,
    },
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler,
};
use windows::{
    Win32::{
        Foundation::{HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::Com::{IStream, StructuredStorage::CreateStreamOnHGlobal},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GWL_STYLE, GWLP_USERDATA,
            GWLP_WNDPROC, GetClientRect, GetMessageW, GetSystemMetrics, GetWindowLongPtrW,
            GetWindowLongW, GetWindowRect, MSG, PostQuitMessage, RegisterClassW, SM_CXSCREEN,
            SM_CYSCREEN, SWP_FRAMECHANGED, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowLongPtrW,
            SetWindowLongW, SetWindowPos, TranslateMessage, WINDOW_EX_STYLE, WM_DESTROY,
            WM_KEYDOWN, WM_NCCREATE, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

fn blocked_stream(msg: &str) -> IStream {
    unsafe {
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).unwrap();
        let bytes = msg.as_bytes();
        let mut written = 0;
        stream
            .Write(
                bytes.as_ptr() as *const _,
                bytes.len() as u32,
                Some(&mut written),
            )
            .unwrap();
        stream
    }
}

struct WindowState {
    controller: ICoreWebView2Controller,
    fullscreen: bool,
    prev_rect: RECT,
    prev_style: i32,
}

fn create_window() -> HWND {
    unsafe {
        let hinstance = HINSTANCE::default();

        let class_name = w!("webview2_client");
        let wc = WNDCLASSW {
            style: Default::default(),
            lpfnWndProc: Some(wnd_proc_setup),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: Default::default(),
            hCursor: Default::default(),
            hbrBackground: Default::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name,
        };

        RegisterClassW(&wc);

        let width = 800;
        let height = 600;

        let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;

        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("WebView2 Client"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(hinstance),
            None,
        )
        .unwrap()
    }
}

fn create_webview2(
    hwnd: HWND,
) -> (
    ICoreWebView2Controller,
    ICoreWebView2,
    ICoreWebView2Environment,
) {
    unsafe {
        let options = CoreWebView2EnvironmentOptions::default();
        options.set_additional_browser_arguments("--disable-frame-rate-limit".to_string());

        let (tx_controller, rx_controller) = std::sync::mpsc::channel();
        let (tx_env, rx_env) = std::sync::mpsc::channel();

        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR::null(),
                    &ICoreWebView2EnvironmentOptions::from(options),
                    &handler,
                )
                .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |err, env| {
                err?;
                let env = env.unwrap();

                tx_env.send(env.clone()).unwrap();

                CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
                    Box::new(move |controller_created_handler| {
                        env.CreateCoreWebView2Controller(hwnd, &controller_created_handler)
                            .map_err(webview2_com::Error::WindowsError)
                    }),
                    Box::new(move |err, controller| {
                        err?;
                        let controller = controller.unwrap();

                        let mut rect = RECT::default();
                        GetClientRect(hwnd, &mut rect).ok();
                        controller.SetBounds(rect).ok();

                        tx_controller.send(controller).unwrap();
                        Ok(())
                    }),
                )
                .unwrap();
                Ok(())
            }),
        )
        .expect("Failed to create WebView2 env");

        let controller = rx_controller.recv().unwrap();
        let env = rx_env.recv().unwrap();
        let webview2 = controller.CoreWebView2().unwrap();

        (controller, webview2, env)
    }
}

extern "system" fn wnd_proc_setup(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc_main as isize);
            return wnd_proc_main(hwnd, msg, wparam, lparam);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

const F11: u32 = 0x7A; // VK_F11
extern "system" fn wnd_proc_main(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        let state = if !ptr.is_null() {
            Some(&mut *ptr)
        } else {
            None
        };

        match msg {
            WM_SIZE => {
                if let Some(state) = state {
                    let mut rect = RECT::default();
                    GetClientRect(hwnd, &mut rect).ok();
                    state.controller.SetBounds(rect).ok();
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if wparam.0 as u32 == F11 {
                    if let Some(state) = state {
                        toggle_fullscreen(hwnd, state);
                    }
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn toggle_fullscreen(hwnd: HWND, state: &mut WindowState) {
    if !state.fullscreen {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect).ok() };
        state.prev_rect = rect;
        state.prev_style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) };

        // Remove window borders & title
        unsafe { SetWindowLongW(hwnd, GWL_STYLE, WS_VISIBLE.0 as i32) };

        // Maximize to full screen
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND::default()),
                0,
                0,
                screen_width,
                screen_height,
                SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
            )
            .unwrap()
        };

        state.fullscreen = true;
    } else {
        unsafe {
            SetWindowLongW(hwnd, GWL_STYLE, state.prev_style);
            SetWindowPos(
                hwnd,
                Some(HWND::default()),
                state.prev_rect.left,
                state.prev_rect.top,
                state.prev_rect.right - state.prev_rect.left,
                state.prev_rect.bottom - state.prev_rect.top,
                SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
            )
            .unwrap();
        };

        state.fullscreen = false;
    }
}

fn main() {
    unsafe {
        let hwnd = create_window();
        let (controller, webview, env) = create_webview2(hwnd);

        let blocked_domains = vec![
            "api.adinplay.com",
            "www.google-analytics.com",
            "www.googletagmanager.com",
            "static.cloudflareinsights.com",
        ];

        let state = Box::new(WindowState {
            controller,
            fullscreen: false,
            prev_rect: RECT::default(),
            prev_style: 0,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        webview
            .AddWebResourceRequestedFilter(PCWSTR::null(), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT)
            .unwrap();

        let script = HSTRING::from(include_str!("script.js"));
        webview
            .AddScriptToExecuteOnDocumentCreated(PCWSTR(script.as_ptr()), None)
            .ok();

        webview.Navigate(w!("https://kirka.io")).ok();

        webview
            .add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                    let mut msg = PWSTR::null();
                    args.unwrap().TryGetWebMessageAsString(&mut msg).ok();
                    if msg.to_string()?.trim_matches('"') == "close" {
                        PostQuitMessage(0);
                    }
                    if msg.to_string()?.trim_matches('"') == "toggle_fullscreen" {
                        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                        if !ptr.is_null() {
                            let state = &mut *ptr;
                            toggle_fullscreen(hwnd, state);
                        }
                    }
                    Ok(())
                })),
                std::ptr::null_mut(),
            )
            .ok();

        webview
            .add_WebResourceRequested(
                &WebResourceRequestedEventHandler::create(Box::new(
                    move |_, args: Option<ICoreWebView2WebResourceRequestedEventArgs>| {
                        let args = args.unwrap();
                        let request = args.Request().unwrap();

                        let mut uri_pwstr = PWSTR::null();
                        request.Uri(&mut uri_pwstr).map_err(|e| e)?;
                        let uri = uri_pwstr.to_string()?;

                        if blocked_domains.iter().any(|domain| uri.contains(domain)) {
                            let stream = blocked_stream("Blocked by client");
                            let response = env
                                .CreateWebResourceResponse(
                                    Some(&stream),
                                    403,
                                    w!("Forbidden"),
                                    w!("Content-Type: text/plain"),
                                )
                                .unwrap();
                            args.SetResponse(&response).unwrap();
                        }

                        Ok(())
                    },
                )),
                std::ptr::null_mut(),
            )
            .unwrap();

        let mut msg: MSG = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
