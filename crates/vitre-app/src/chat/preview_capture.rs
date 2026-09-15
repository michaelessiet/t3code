//! WKWebView snapshots are asynchronous; never block the main run loop waiting
//! for WebKit's completion handler.
#[cfg(target_os = "macos")]
pub(super) fn snapshot(
    view: &wry::WebView,
) -> tokio::sync::oneshot::Receiver<Result<Vec<u8>, String>> {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use wry::WebViewExtMacOS as _;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::cell::RefCell::new(Some(tx));
    let callback = block2::RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
        let result = (|| {
            // WebKit owns these callback arguments for the duration of the call.
            if let Some(error) = unsafe { error.as_ref() } {
                return Err(error.localizedDescription().to_string());
            }
            let image = unsafe { image.as_ref() }.ok_or("WebKit returned no snapshot")?;
            let tiff = image
                .TIFFRepresentation()
                .ok_or("Could not encode the snapshot")?;
            let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)
                .ok_or("Could not read the snapshot bitmap")?;
            let png = unsafe {
                bitmap.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }
            .ok_or("Could not encode PNG")?;
            Ok(png.to_vec())
        })();
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(result);
        }
    });
    unsafe {
        view.webview()
            .takeSnapshotWithConfiguration_completionHandler(None, &callback)
    };
    rx
}

/// Deliver keys through WebKit's native responder, including default editing
/// actions that synthetic DOM KeyboardEvents cannot perform.
#[cfg(target_os = "macos")]
pub(super) fn press(view: &wry::WebView, key: &str, modifiers: &[String]) -> Result<(), String> {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSPoint, NSString};
    use wry::WebViewExtMacOS as _;
    let (chars, code) = match key {
        "Enter" => ("\r", 36),
        "Tab" => ("\t", 48),
        "Escape" => ("\u{1b}", 53),
        "Backspace" => ("\u{7f}", 51),
        "Delete" => ("\u{f728}", 117),
        "ArrowUp" => ("\u{f700}", 126),
        "ArrowDown" => ("\u{f701}", 125),
        "ArrowLeft" => ("\u{f702}", 123),
        "ArrowRight" => ("\u{f703}", 124),
        "Home" => ("\u{f729}", 115),
        "End" => ("\u{f72b}", 119),
        "PageUp" => ("\u{f72c}", 116),
        "PageDown" => ("\u{f72d}", 121),
        value if value.chars().count() == 1 => (value, 0),
        _ => return Err(format!("Unsupported native key: {key}")),
    };
    let mut flags = NSEventModifierFlags::empty();
    for modifier in modifiers {
        flags |= match modifier.as_str() {
            "Meta" => NSEventModifierFlags::Command,
            "Control" => NSEventModifierFlags::Control,
            "Alt" => NSEventModifierFlags::Option,
            "Shift" => NSEventModifierFlags::Shift,
            _ => return Err("Unknown keyboard modifier".into()),
        };
    }
    view.focus().map_err(|e| e.to_string())?;
    let window = view
        .webview()
        .window()
        .ok_or("Browser has no native window")?;
    let responder = window
        .firstResponder()
        .ok_or("Browser has no keyboard responder")?;
    let chars = NSString::from_str(chars);
    for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let event=NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(kind,NSPoint::new(0.,0.),flags,0.,window.windowNumber(),None,&chars,&chars,false,code).ok_or("Could not create native key event")?;
        if kind == NSEventType::KeyDown {
            responder.keyDown(&event);
        } else {
            responder.keyUp(&event);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(super) fn snapshot(
    _: &wry::WebView,
) -> tokio::sync::oneshot::Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = tx.send(Err(
        "Native snapshots are unavailable on this platform.".into()
    ));
    rx
}
