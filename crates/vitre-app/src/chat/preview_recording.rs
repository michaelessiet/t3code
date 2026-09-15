//! Bounded native recording: capture on the main thread, encode on a worker.
//! GIF keeps artifacts playable without bundling an external video encoder.
use super::preview::PreviewPanel;
use gpui::{Context, Task};
use serde_json::{Value, json};

pub(super) struct Recording {
    pub started_at: String,
    capture: Task<()>,
    finished: tokio::sync::oneshot::Receiver<Result<Value, String>>,
}

impl Recording {
    pub fn start(tab_id: String, cx: &mut Context<PreviewPanel>) -> Self {
        let id = super::fresh_id("recording");
        let path = crate::vitre_home()
            .join("preview-artifacts")
            .join(format!("{id}.gif"));
        let started_at = chrono::Utc::now().to_rfc3339();
        let created = started_at.clone();
        let (frames_tx, mut frames_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
        let (finished_tx, finished) = tokio::sync::oneshot::channel();
        cx.background_executor().spawn(async move {
            let result=async {
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e|e.to_string())?;
                let file=std::fs::File::create(&path).map_err(|e|e.to_string())?;
                let mut encoder=image::codecs::gif::GifEncoder::new_with_speed(file,20);
                encoder.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(|e|e.to_string())?;
                let mut dimensions=None;let mut count=0;
                while let Some(png)=frames_rx.recv().await {
                    let image=image::load_from_memory_with_format(&png,image::ImageFormat::Png).map_err(|e|e.to_string())?;
                    let image=image.resize(960,960,image::imageops::FilterType::Triangle).to_rgba8();
                    let(w,h)=*dimensions.get_or_insert(image.dimensions());
                    let image=if image.dimensions()==(w,h){image}else{image::imageops::resize(&image,w,h,image::imageops::FilterType::Triangle)};
                    encoder.encode_frame(image::Frame::from_parts(image,0,0,image::Delay::from_numer_denom_ms(250,1))).map_err(|e|e.to_string())?;
                    count+=1;
                }
                drop(encoder);
                if count==0{return Err("Recording contained no frames".to_string());}
                let size=std::fs::metadata(&path).map_err(|e|e.to_string())?.len();
                Ok(json!({"id":id,"tabId":tab_id,"path":path,"mimeType":"image/gif","sizeBytes":size,"createdAt":created}))
            }.await;
            let _=finished_tx.send(result);
        }).detach();
        let executor = cx.background_executor().clone();
        let capture=cx.spawn(async move |this,cx| {
            // Two minutes at 4 FPS is the capture budget. The encoder's bounded
            // queue applies backpressure, so a slow disk cannot accumulate RAM.
            for _ in 0..480 {
                let Ok(Some(frame))=this.update(cx,|panel,cx|panel.native_snapshot(cx))else{break;};
                let result=tokio::select! {r=frame=>r,_=executor.timer(std::time::Duration::from_secs(10))=>break};
                match result {Ok(Ok(frame))=>{if frames_tx.send(frame).await.is_err(){break;}},_=>break}
                executor.timer(std::time::Duration::from_millis(250)).await;
            }
        });
        Self {
            started_at,
            capture,
            finished,
        }
    }

    pub fn stop(self) -> tokio::sync::oneshot::Receiver<Result<Value, String>> {
        drop(self.capture);
        self.finished
    }
}
