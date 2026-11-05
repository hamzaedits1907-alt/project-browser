//! "Copy text from image": the picture is captured from the page exactly as it is shown (through DevTools'
//! `Page.captureScreenshot`, so no second download and no cross-site restrictions), and Windows' own text recognition
//! (`Windows.Media.Ocr`, the engine behind the Snipping Tool's text actions) reads it, on this computer, in the
//! languages installed in Windows. Nothing is sent anywhere.

use webview2_com::*;
use windows::core::HSTRING;

use crate::tabs::TabId;

/// Script finding the picture `src` in the page: its box in page coordinates (CSS pixels), or null.
fn locate_script(src: &str) -> String {
    let s = serde_json::to_string(src).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(()=>{{const s={s};const im=[...document.images].find(i=>i.currentSrc===s||i.src===s)||[...document.querySelectorAll('picture img,img')].find(i=>(i.currentSrc||'').split('?')[0]===s.split('?')[0]);\
if(!im)return null;const r=im.getBoundingClientRect();if(r.width<4||r.height<4)return null;\
return {{x:r.left+scrollX,y:r.top+scrollY,w:r.width,h:r.height}}}})()"
    )
}

/// Captures picture `src` of tab `id` as PNG (scaled so text is large enough for recognition). UI thread.
pub fn capture_image(id: TabId, src: String, reply: tokio::sync::oneshot::Sender<Option<Vec<u8>>>) {
    let Some(wv) = crate::native::tab_webview(id) else {
        let _ = reply.send(None);
        return;
    };
    let wv2 = wv.clone();
    unsafe {
        let _ = wv.ExecuteScript(
            &HSTRING::from(locate_script(&src)),
            &ExecuteScriptCompletedHandler::create(Box::new(move |res, json| {
                let rect = res
                    .ok()
                    .and_then(|_| serde_json::from_str::<serde_json::Value>(&json).ok())
                    .and_then(|v| Some((v.get("x")?.as_f64()?, v.get("y")?.as_f64()?, v.get("w")?.as_f64()?, v.get("h")?.as_f64()?)));
                let Some((x, y, w, h)) = rect else {
                    let _ = reply.send(None);
                    return Ok(());
                };
                // Windows' recognizer reads up to about 2600 pixels per side and likes text a little larger.
                let scale = (2400.0 / w.max(h)).clamp(0.25, 2.0);
                let params = serde_json::json!({
                    "format": "png",
                    "captureBeyondViewport": true,
                    "clip": { "x": x, "y": y, "width": w, "height": h, "scale": scale }
                })
                .to_string();
                let _ = wv2.CallDevToolsProtocolMethod(
                    windows::core::w!("Page.captureScreenshot"),
                    &HSTRING::from(params),
                    &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, json| {
                        use base64::Engine as _;
                        let png = res.ok().and_then(|_| serde_json::from_str::<serde_json::Value>(&json).ok()).and_then(|v| {
                            v.get("data").and_then(|d| d.as_str()).and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok())
                        });
                        let _ = reply.send(png);
                        Ok(())
                    })),
                );
                Ok(())
            })),
        );
    }
}

/// Reads the text in a PNG with Windows' text recognition. Blocking; call it off the UI thread.
pub fn recognize_png(png: &[u8]) -> std::result::Result<String, String> {
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    let err = |e: windows::core::Error| e.message().to_string();
    unsafe {
        // WinRT on this worker thread (harmless if it is initialized already).
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| "Windows has no text recognition for your languages. Add a language with its \"Optical character recognition\" feature in Windows Settings > Time & language.".to_string())?;
    let stream = InMemoryRandomAccessStream::new().map_err(err)?;
    let writer = DataWriter::CreateDataWriter(&stream).map_err(err)?;
    writer.WriteBytes(png).map_err(err)?;
    writer.StoreAsync().map_err(err)?.join().map_err(err)?;
    writer.FlushAsync().map_err(err)?.join().map_err(err)?;
    writer.DetachStream().map_err(err)?;
    stream.Seek(0).map_err(err)?;
    let decoder = BitmapDecoder::CreateAsync(&stream).map_err(err)?.join().map_err(err)?;
    let bitmap = decoder.GetSoftwareBitmapAsync().map_err(err)?.join().map_err(err)?;
    let bitmap = SoftwareBitmap::ConvertWithAlpha(&bitmap, BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied).map_err(err)?;
    let result = engine.RecognizeAsync(&bitmap).map_err(err)?.join().map_err(err)?;
    let lines = result.Lines().map_err(err)?;
    let mut out = Vec::new();
    for line in lines {
        if let Ok(t) = line.Text() {
            out.push(t.to_string_lossy());
        }
    }
    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_locate_script_quotes_the_address() {
        let s = locate_script("https://x.test/a\"b</script>.png");
        assert!(s.contains(r#"const s="https://x.test/a\"b</script>.png""#));
    }
}
