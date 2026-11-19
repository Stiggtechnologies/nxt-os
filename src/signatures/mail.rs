//! TLS-only mail delivery. Credentials are AEAD-encrypted with a deployment key;
//! callers receive masked configuration and acceptance receipts, never secrets.
use super::*;
use lettre::{AsyncSmtpTransport,AsyncTransport,Message,Tokio1Executor,transport::smtp::authentication::Credentials};
use ring::{aead,rand::{SecureRandom,SystemRandom}};
use serde::{Deserialize,Serialize};
use std::time::Duration;
const CONFIG:&str="signatures:mail:configuration:v1";
const AAD:&[u8]=b"nxt-signature-mail/v1";
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailInput {
    pub hostname:String,pub port:u16,pub sender:String,pub username:String,
    pub password:String,pub enabled:bool,pub expected_revision:String,
}
fn key() -> SResult<Vec<u8>> {
    if std::env::var("VERCEL_ENV").ok().as_deref()==Some("preview"){return Err(conflict("Mailbox connections are disabled on preview deployments"));}
    let bytes=std::env::var("NXT_ESIGN_MAIL_KEY").ok().and_then(|v|hex::decode(v).ok()).filter(|v|v.len()==32);
    bytes.ok_or_else(||conflict("Secure mail storage is not configured on this deployment"))
}
fn crypt_key(bytes:&[u8])->SResult<aead::LessSafeKey>{
    aead::UnboundKey::new(&aead::AES_256_GCM,bytes).map(aead::LessSafeKey::new).map_err(|_|storage_error(String::new()))
}
fn seal(bytes:&[u8],plain:&[u8])->SResult<String>{
    let mut nonce=[0u8;12];SystemRandom::new().fill(&mut nonce).map_err(|_|storage_error(String::new()))?;
    let mut encrypted=plain.to_vec();crypt_key(bytes)?.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce),aead::Aad::from(AAD),&mut encrypted).map_err(|_|storage_error(String::new()))?;
    Ok(hex::encode([nonce.as_slice(),encrypted.as_slice()].concat()))
}
fn open(bytes:&[u8],cipher:&str)->SResult<Vec<u8>>{
    let mut encrypted=hex::decode(cipher).map_err(|_|storage_error(String::new()))?;
    if encrypted.len()<28{return Err(storage_error(String::new()));}
    let nonce:[u8;12]=encrypted[..12].try_into().map_err(|_|storage_error(String::new()))?;
    crypt_key(bytes)?.open_in_place(aead::Nonce::assume_unique_for_key(nonce),aead::Aad::from(AAD),&mut encrypted[12..]).map(|v|v.to_vec()).map_err(|_|storage_error(String::new()))
}
fn validate(v:&MailInput)->SResult<()> {
    // Restrict SMTP to the verified account's provider choices, avoiding arbitrary
    // TCP connections or private-network destinations from a mail-settings form.
    if !["smtp.office365.com","s1363.use1.mysecurecloudhost.com"].contains(&v.hostname.as_str()) || ![465,587].contains(&v.port){return Err(bad("Choose a supported SMTP server and TLS port 465 or 587"));}
    if v.hostname=="smtp.office365.com"&&v.port!=587{return Err(bad("Microsoft 365 requires STARTTLS on port 587"));}
    if v.sender.parse::<lettre::message::Mailbox>().is_err()||v.username!=v.sender||v.password.is_empty()||v.password.len()>2048||v.password.chars().any(|c|c=='\r'||c=='\n') {return Err(bad("Enter a sender mailbox, the same SMTP username and its authorized SMTP credential"));}
    Ok(())
}
fn transport(v:&MailInput)->SResult<AsyncSmtpTransport<Tokio1Executor>> {
    validate(v)?;
    let builder=if v.port==465 {AsyncSmtpTransport::<Tokio1Executor>::relay(&v.hostname)} else {AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&v.hostname)};
    Ok(builder.map_err(|_|bad("SMTP TLS setup failed"))?.port(v.port).credentials(Credentials::new(v.username.clone(),v.password.clone())).timeout(Some(Duration::from_secs(15))).build())
}
fn revision(raw:Option<&str>)->String {raw.map(|v|hex::encode(Sha256::digest(v.as_bytes()))).unwrap_or_default()}
impl Signatures {
    async fn mail_configuration(&self)->SResult<Option<MailInput>> {
        if std::env::var("VERCEL_ENV").ok().as_deref()==Some("preview"){return Ok(None);}
        let Some(raw)=self.store.get(CONFIG).await.map_err(storage_error)? else{return Ok(None)};
        let plain=open(&key()?,&raw)?;
        serde_json::from_slice(&plain).map(Some).map_err(|_|storage_error(String::new()))
    }
    pub async fn mail_settings(&self)->SResult<Value>{
        let raw=self.store.get(CONFIG).await.map_err(storage_error)?;
        let config=if raw.is_some(){self.mail_configuration().await?}else{None};
        Ok(json!({"configured":config.is_some(),"secure_storage_ready":key().is_ok(),"revision":revision(raw.as_deref()),
            "hostname":config.as_ref().map(|v|&v.hostname),"port":config.as_ref().map(|v|v.port),
            "sender":config.as_ref().map(|v|&v.sender),"enabled":config.as_ref().is_some_and(|v|v.enabled),
            "credential_present":config.is_some(),"credential_returned":false}))
    }
    pub async fn save_mail_settings(&self,mut input:MailInput)->SResult<Value>{
        let encryption=key()?;
        let old=self.store.get(CONFIG).await.map_err(storage_error)?;
        if input.expected_revision!=revision(old.as_deref()){return Err(conflict("Mail settings changed. Reload before saving"));}
        if input.password.is_empty(){if let Some(previous)=self.mail_configuration().await? {
            if previous.sender==input.sender&&previous.username==input.username&&previous.hostname==input.hostname {input.password=previous.password;}
        }}
        validate(&input)?;
        // Check TLS and authentication only. No test email or document is sent.
        if !transport(&input)?.test_connection().await.unwrap_or(false){return Err(conflict("SMTP connection or authentication failed. Check the provider's SMTP settings; no email was sent"));}
        input.expected_revision.clear();
        let encrypted=seal(&encryption,&serde_json::to_vec(&input).map_err(|_|storage_error(String::new()))?)?;
        if !self.store.compare_exchange(CONFIG,old.as_deref(),&encrypted).await.map_err(storage_error)?{return Err(conflict("Mail settings changed while checking the connection. Reload"));}
        self.mail_settings().await
    }
    pub async fn mail_delivery_mode(&self)->SResult<&'static str>{
        if self.mail_configuration().await?.is_some_and(|v|v.enabled){Ok("smtp")}else{esign::delivery_mode()}
    }
    pub async fn mail_acceptance(&self,id:&str,to:&str,subject:&str,body:&str)->SResult<Value>{
        let config=self.mail_configuration().await?.filter(|v|v.enabled).ok_or_else(||conflict("The sending mailbox is not connected"))?;
        let message=Message::builder().from(config.sender.parse().map_err(|_|bad("Sender mailbox is invalid"))?).to(to.parse().map_err(|_|bad("Recipient mailbox is invalid"))?).subject(subject).body(body.to_string()).map_err(|_|bad("Email could not be prepared"))?;
        let mail=transport(&config)?;
        self.record_delivery(id,async move {mail.send(message).await.is_ok()}).await
    }
    async fn record_delivery(&self,id:&str,send:impl std::future::Future<Output=bool>)->SResult<Value>{
        let receipt_key=format!("signatures:mail:receipt:{}",hex::encode(Sha256::digest(id.as_bytes())));
        let pending=json!({"status":"sending","at":now(),"email_sent":false}).to_string();
        if !self.store.set_nx(&receipt_key,&pending).await.map_err(storage_error)? {
            let old=self.store.get(&receipt_key).await.map_err(storage_error)?.ok_or_else(missing)?;
            return serde_json::from_str(&old).map_err(|_|storage_error(String::new()));
        }
        // A timeout or interrupted acceptance can be ambiguous; never retry it
        // automatically. A reviewed new-link action issues a new message identity.
        let accepted=send.await;
        let result=json!({"status":if accepted{"accepted_by_mail_server"}else{"delivery_unconfirmed"},"at":now(),"email_sent":accepted,"delivered_to_inbox":false});
        if !self.store.compare_exchange(&receipt_key,Some(&pending),&result.to_string()).await.map_err(storage_error)?{return Ok(json!({"email_sent":false,"status":"delivery_unconfirmed"}));}
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn authenticated_encryption_rejects_tamper_and_wrong_keys(){
  let key=[7u8;32];let first=seal(&key,b"private smtp credential").ok().unwrap();let next=seal(&key,b"private smtp credential").ok().unwrap();assert_ne!(first,next);
  assert_eq!(open(&key,&first).ok().unwrap(),b"private smtp credential");assert!(open(&[8u8;32],&first).is_err());
  let mut bad=hex::decode(first).unwrap();bad[15]^=1;assert!(open(&key,&hex::encode(bad)).is_err());
 }
 #[test] fn smtp_rejects_untrusted_servers_plaintext_ports_and_sender_impersonation(){
  let mut v=MailInput{hostname:"smtp.office365.com".into(),port:587,sender:"info@example.com".into(),username:"info@example.com".into(),password:"synthetic".into(),enabled:false,expected_revision:String::new()};
  assert!(validate(&v).is_ok());v.port=25;assert!(validate(&v).is_err());v.port=587;v.hostname="127.0.0.1".into();assert!(validate(&v).is_err());v.hostname="s1363.use1.mysecurecloudhost.com".into();v.username="other@example.com".into();assert!(validate(&v).is_err());
 }
 #[tokio::test] async fn simultaneous_delivery_replays_never_send_twice_and_failures_stay_unconfirmed(){
  use std::sync::atomic::{AtomicUsize,Ordering};
  let s=Signatures::new(Store::memory());let calls=AtomicUsize::new(0);
  let attempt=||async{calls.fetch_add(1,Ordering::SeqCst);tokio::task::yield_now().await;true};
  let (a,b)=tokio::join!(s.record_delivery("same-request",attempt()),s.record_delivery("same-request",attempt()));
  assert_eq!(calls.load(Ordering::SeqCst),1);assert!(a.is_ok()&&b.is_ok());
  let receipt=s.record_delivery("same-request",attempt()).await.ok().unwrap();assert_eq!(receipt["email_sent"],true);assert_eq!(calls.load(Ordering::SeqCst),1);
  let failed=s.record_delivery("failed-request",async{false}).await.ok().unwrap();assert_eq!(failed["email_sent"],false);assert_eq!(failed["status"],"delivery_unconfirmed");
  let replay=s.record_delivery("failed-request",attempt()).await.ok().unwrap();assert_eq!(replay["email_sent"],false);assert_eq!(calls.load(Ordering::SeqCst),1);
 }
}
