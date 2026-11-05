//! Small pictures of tabs for the hover cards over the tab strip. A tab is captured when it goes to the background (the
//! last moment it is drawn) and shrunk to `WIDTH` pixels wide with Windows' imaging component, so a picture is a few
//! kilobytes; at most `KEEP` are kept, the least recently captured go first.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::tabs::TabId;

pub const WIDTH: u32 = 320;
const KEEP: usize = 30;

/// Tab -> (small JPEG, when it was captured).
type ThumbMap = HashMap<TabId, (Vec<u8>, Instant)>;
static THUMBS: Mutex<Option<ThumbMap>> = Mutex::new(None);

/// The kept picture of tab `id`, if any.
pub fn get(id: TabId) -> Option<Vec<u8>> {
    THUMBS.lock().ok()?.as_ref()?.get(&id).map(|(b, _)| b.clone())
}

pub fn forget(id: TabId) {
    if let Ok(mut t) = THUMBS.lock() {
        if let Some(m) = t.as_mut() {
            m.remove(&id);
        }
    }
}

fn keep(id: TabId, jpeg: Vec<u8>) {
    if let Ok(mut t) = THUMBS.lock() {
        let m = t.get_or_insert_with(HashMap::new);
        m.insert(id, (jpeg, Instant::now()));
        while m.len() > KEEP {
            let Some(oldest) = m.iter().min_by_key(|(_, (_, at))| *at).map(|(k, _)| *k) else { break };
            m.remove(&oldest);
        }
    }
}

/// Captures tab `id` now and keeps a small picture of it (UI thread; the shrinking happens on a worker thread).
/// `reply`, if given, gets the small picture too.
pub fn capture(id: TabId, reply: Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::native::capture_preview(id, tx);
    std::thread::spawn(move || {
        let small = rx.blocking_recv().ok().flatten().and_then(|full| shrink(&full).ok());
        if let Some(s) = &small {
            keep(id, s.clone());
        }
        if let Some(r) = reply {
            let _ = r.send(small);
        }
    });
}

/// Re-encodes a picture as a JPEG `WIDTH` pixels wide. Blocking; not on the UI thread.
pub fn shrink(image: &[u8]) -> std::result::Result<Vec<u8>, String> {
    use windows::Graphics::Imaging::{BitmapDecoder, BitmapEncoder, BitmapInterpolationMode};
    use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    let err = |e: windows::core::Error| e.message().to_string();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let input = InMemoryRandomAccessStream::new().map_err(err)?;
    let writer = DataWriter::CreateDataWriter(&input).map_err(err)?;
    writer.WriteBytes(image).map_err(err)?;
    writer.StoreAsync().map_err(err)?.join().map_err(err)?;
    writer.FlushAsync().map_err(err)?.join().map_err(err)?;
    writer.DetachStream().map_err(err)?;
    input.Seek(0).map_err(err)?;
    let decoder = BitmapDecoder::CreateAsync(&input).map_err(err)?.join().map_err(err)?;
    let (w, h) = (decoder.PixelWidth().map_err(err)?, decoder.PixelHeight().map_err(err)?);
    if w == 0 || h == 0 {
        return Err("empty picture".into());
    }
    let bitmap = decoder.GetSoftwareBitmapAsync().map_err(err)?.join().map_err(err)?;
    let output = InMemoryRandomAccessStream::new().map_err(err)?;
    let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::JpegEncoderId().map_err(err)?, &output).map_err(err)?.join().map_err(err)?;
    encoder.SetSoftwareBitmap(&bitmap).map_err(err)?;
    let transform = encoder.BitmapTransform().map_err(err)?;
    let width = WIDTH.min(w);
    transform.SetScaledWidth(width).map_err(err)?;
    transform.SetScaledHeight(((h as u64 * width as u64) / w as u64).max(1) as u32).map_err(err)?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant).map_err(err)?;
    encoder.FlushAsync().map_err(err)?.join().map_err(err)?;
    let size = output.Size().map_err(err)? as u32;
    let reader = DataReader::CreateDataReader(&output.GetInputStreamAt(0).map_err(err)?).map_err(err)?;
    reader.LoadAsync(size).map_err(err)?.join().map_err(err)?;
    let mut out = vec![0u8; size as usize];
    reader.ReadBytes(&mut out).map_err(err)?;
    Ok(out)
}
