//! DocuSign completed-envelope ZIP: one or more document PDFs plus `Summary.pdf`
//! (the Certificate of Completion). Reads the ZIP central directory only.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZipEntry { pub name: String, pub compressed: u32, pub size: u32 }

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Envelope { pub documents: Vec<ZipEntry>, pub certificate: Option<ZipEntry>, pub other: Vec<ZipEntry> }

impl Envelope { pub fn is_complete(&self) -> bool { self.certificate.is_some() && !self.documents.is_empty() } }

fn u16a(b: &[u8], o: usize) -> Option<usize> { b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize) }
fn u32a(b: &[u8], o: usize) -> Option<u32> { b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])) }

pub fn list_zip(b: &[u8]) -> Result<Vec<ZipEntry>, String> {
    let eocd = (0..b.len().saturating_sub(21)).rev()
        .find(|&i| b[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or("not a zip (no EOCD)")?;
    let n = u16a(b, eocd + 10).ok_or("bad EOCD")?;
    let mut p = u32a(b, eocd + 16).ok_or("bad EOCD")? as usize;
    let mut out = Vec::new();
    for _ in 0..n {
        if b.get(p..p + 4) != Some(&[0x50, 0x4b, 0x01, 0x02][..]) { return Err("bad central directory".into()); }
        let (cs, us) = (u32a(b, p + 20).ok_or("trunc")?, u32a(b, p + 24).ok_or("trunc")?);
        let (nl, el, cl) = (u16a(b, p + 28).ok_or("trunc")?, u16a(b, p + 30).ok_or("trunc")?, u16a(b, p + 32).ok_or("trunc")?);
        let name = String::from_utf8_lossy(b.get(p + 46..p + 46 + nl).ok_or("trunc")?).into_owned();
        out.push(ZipEntry { name, compressed: cs, size: us });
        p += 46 + nl + el + cl;
    }
    Ok(out)
}

pub fn parse_envelope_zip(b: &[u8]) -> Result<Envelope, String> {
    let mut env = Envelope { documents: vec![], certificate: None, other: vec![] };
    for e in list_zip(b)? {
        if e.name.ends_with('/') { continue; }
        let base = e.name.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
        if base == "summary.pdf" { env.certificate = Some(e) }
        else if base.ends_with(".pdf") { env.documents.push(e) } else { env.other.push(e) }
    }
    Ok(env)
}

/// Single loose PDF named like a DocuSign download (no certificate available).
pub fn is_docusign_pdf_name(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".pdf") && (l.starts_with("complete_with_docusign") || l.starts_with("please_docusign") || l.contains("docusign"))
}

#[cfg(test)]
pub(crate) fn make_zip(names: &[(&str, &[u8])]) -> Vec<u8> {
    let (mut out, mut cd) = (Vec::new(), Vec::new());
    for (n, d) in names {
        let off = out.len() as u32;
        let mut h = vec![0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        h.extend((d.len() as u32).to_le_bytes()); h.extend((d.len() as u32).to_le_bytes());
        h.extend((n.len() as u16).to_le_bytes()); h.extend([0, 0]); h.extend(n.as_bytes()); h.extend(*d);
        out.extend(h);
        let mut c = vec![0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        c.extend((d.len() as u32).to_le_bytes()); c.extend((d.len() as u32).to_le_bytes());
        c.extend((n.len() as u16).to_le_bytes()); c.extend([0u8; 12]); c.extend(off.to_le_bytes()); c.extend(n.as_bytes());
        cd.extend(c);
    }
    let cdo = out.len() as u32; let cdl = cd.len() as u32; out.extend(cd);
    out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
    out.extend((names.len() as u16).to_le_bytes()); out.extend((names.len() as u16).to_le_bytes());
    out.extend(cdl.to_le_bytes()); out.extend(cdo.to_le_bytes()); out.extend([0, 0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn completed_envelope() {
        let z = make_zip(&[("Engagement_Letter.pdf", b"%PDF-1.4 x"), ("Summary.pdf", b"%PDF-1.4 y")]);
        let e = parse_envelope_zip(&z).unwrap();
        assert!(e.is_complete()); assert_eq!(e.documents[0].name, "Engagement_Letter.pdf"); assert_eq!(e.certificate.unwrap().size, 10);
    }
    #[test] fn missing_certificate() {
        let e = parse_envelope_zip(&make_zip(&[("a.pdf", b"x")])).unwrap(); assert!(!e.is_complete());
    }
    #[test] fn not_zip() { assert!(parse_envelope_zip(b"%PDF").is_err()); }
    #[test] fn loose_names() { assert!(is_docusign_pdf_name("Complete_with_DocuSign_doc1.pdf")); assert!(!is_docusign_pdf_name("x.pdf")); }
}
