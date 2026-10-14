//! Local SAPI voice on one COM-owned worker. Never starts a shell or uses a service.
use serde::Serialize;
use std::sync::{mpsc, OnceLock};
use windows::{
    core::{w, PCWSTR},
    Win32::{Media::Speech::*, System::Com::*},
};

#[derive(Serialize)]
pub struct Status {
    pub speaking: bool,
    pub voice: String,
    pub characters: usize,
    pub elapsed_ms: f64,
}
struct Request {
    query: bool,
    text: String,
    language: String,
    volume: u16,
    reply: Option<mpsc::Sender<Result<Status, String>>>,
}
static WORKER: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
fn sender() -> &'static mpsc::Sender<Request> {
    WORKER.get_or_init(||{
    let (tx,rx)=mpsc::channel::<Request>();
    std::thread::spawn(move||unsafe{
        let initialized=CoInitializeEx(None,COINIT_MULTITHREADED).ok();
        let voice:Result<ISpVoice,_>=CoCreateInstance(&SpVoice,None,CLSCTX_ALL);
        for request in rx {
            let started=std::time::Instant::now();
            let result=(||->Result<Status,String>{
                let v=voice.as_ref().map_err(|e|format!("Windows speech unavailable: {e}"))?;
                if request.query{let mut status=SPVOICESTATUS::default();v.GetStatus(&mut status,std::ptr::null_mut()).map_err(|e|e.to_string())?;return Ok(Status{speaking:status.dwRunningState==SPRS_IS_SPEAKING.0 as u32,voice:"Windows speech status".into(),characters:0,elapsed_ms:started.elapsed().as_secs_f64()*1000.})}
                if request.text.is_empty(){v.Speak(w!(""),(SPF_ASYNC.0|SPF_PURGEBEFORESPEAK.0|SPF_IS_NOT_XML.0) as u32,None).map_err(|e|e.to_string())?;return Ok(Status{speaking:false,voice:"Stopped".into(),characters:0,elapsed_ms:started.elapsed().as_secs_f64()*1000.})}
                let category:ISpObjectTokenCategory=CoCreateInstance(&SpObjectTokenCategory,None,CLSCTX_ALL).map_err(|e|e.to_string())?;
                category.SetId(SPCAT_VOICES,false).map_err(|e|e.to_string())?;
                let filter:Vec<u16>=format!("Language={}",if request.language=="de"{"407"}else{"409"}).encode_utf16().chain(Some(0)).collect();
                let tokens=category.EnumTokens(PCWSTR(filter.as_ptr()),PCWSTR::null()).map_err(|e|e.to_string())?;
                let mut count=0;tokens.GetCount(&mut count).map_err(|e|e.to_string())?;
                if count==0{return Err(format!("No installed local {} SAPI voice. Choose an installed language in Settings.",request.language))}
                let token=tokens.Item(0).map_err(|e|e.to_string())?;
                let description=token.GetStringValue(PCWSTR::null()).map_err(|e|e.to_string())?;
                let name=description.to_string().unwrap_or_else(|_|"Local Windows voice".into());
                CoTaskMemFree(Some(description.0.cast()));
                v.SetVoice(&token).map_err(|e|e.to_string())?;
                v.SetVolume(request.volume.min(100)).map_err(|e|e.to_string())?;
                v.SetRate(0).map_err(|e|e.to_string())?;
                let text:Vec<u16>=request.text.encode_utf16().chain(Some(0)).collect();
                v.Speak(PCWSTR(text.as_ptr()),(SPF_ASYNC.0|SPF_PURGEBEFORESPEAK.0|SPF_IS_NOT_XML.0) as u32,None).map_err(|e|e.to_string())?;
                Ok(Status{speaking:true,voice:name,characters:request.text.chars().count(),elapsed_ms:started.elapsed().as_secs_f64()*1000.})
            })();
            if let Some(reply)=request.reply{let _=reply.send(result);}
        }
        drop(voice);if initialized.is_ok(){CoUninitialize()}
    });tx
})
}
#[tauri::command]
pub async fn speak_local(
    text: String,
    language: String,
    volume: f64,
    query: Option<bool>,
) -> Result<Status, String> {
    if text.chars().count() > 1200 {
        return Err("Coach speech is limited to 1200 characters".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = mpsc::channel();
        sender()
            .send(Request {
                query: query.unwrap_or(false),
                text,
                language,
                volume: (volume.clamp(0., 1.) * 100.) as u16,
                reply: Some(tx),
            })
            .map_err(|_| "Voice worker stopped")?;
        rx.recv_timeout(std::time::Duration::from_secs(3))
            .map_err(|_| "Windows voice did not respond in time")?
    })
    .await
    .map_err(|e| e.to_string())?
}
pub fn stop() {
    if let Some(tx) = WORKER.get() {
        let _ = tx.send(Request {
            query: false,
            text: String::new(),
            language: "en".into(),
            volume: 0,
            reply: None,
        });
    }
}
