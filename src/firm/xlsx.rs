//! Small, dependency-free OpenXML writer for our bounded native report CSVs.
//! ZIP entries are stored (method 0), with CRC32 and a standard central directory.
//! Text is always inlineStr; imported text can never become an Excel formula.

const MAX_CSV: usize = 1024 * 1024;
const MAX_WORKBOOK: usize = 10 * 1024 * 1024;

pub(super) fn parse_csv(csv: &str) -> Result<Vec<Vec<String>>, String> {
    if csv.len() > MAX_CSV { return Err("CSV exceeds the 1 MiB report limit".into()); }
    let bytes = csv.as_bytes();
    let mut rows = Vec::new(); let mut row = Vec::new(); let mut field = Vec::new();
    // 0: field start, 1: unquoted, 2: quoted, 3: closing quote.
    let mut state = 0u8; let mut dirty = false; let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if state == 2 {
            if b == b'"' { state = 3; } else { field.push(b); }
        } else if state == 3 && b == b'"' {
            field.push(b'"'); state = 2;
        } else if b == b',' {
            push_field(&mut row, &mut field)?; state = 0; dirty = true;
        } else if b == b'\r' || b == b'\n' {
            push_field(&mut row, &mut field)?;
            push_row(&mut rows, &mut row)?;
            state = 0; dirty = false;
            if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') { i += 1; }
        } else if state == 3 {
            return Err("unexpected character after quoted CSV field".into());
        } else if b == b'"' {
            if state != 0 { return Err("quote inside unquoted CSV field".into()); }
            state = 2; dirty = true;
        } else {
            field.push(b); state = 1; dirty = true;
        }
        i += 1;
    }
    if state == 2 { return Err("unterminated quoted CSV field".into()); }
    if dirty || !row.is_empty() || !field.is_empty() { push_field(&mut row, &mut field)?; push_row(&mut rows, &mut row)?; }
    if rows.is_empty() { return Err("report CSV has no header".into()); }
    let width = rows[0].len();
    if rows.iter().any(|r| r.len() != width) { return Err("inconsistent CSV column count".into()); }
    Ok(rows)
}

fn push_field(row: &mut Vec<String>, field: &mut Vec<u8>) -> Result<(), String> {
    if row.len() >= 128 { return Err("report has too many columns".into()); }
    row.push(String::from_utf8(std::mem::take(field)).map_err(|_| "CSV field is not valid UTF-8")?);
    Ok(())
}
fn push_row(rows: &mut Vec<Vec<String>>, row: &mut Vec<String>) -> Result<(), String> {
    if rows.len() >= 20000 { return Err("report has too many rows".into()); }
    rows.push(std::mem::take(row)); Ok(())
}

fn xml(value: &str) -> String {
    let mut output = String::new();
    for c in value.chars() {
        match c {
            '&' => output.push_str("&amp;"), '<' => output.push_str("&lt;"), '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"), '\'' => output.push_str("&apos;"),
            '\r' => output.push_str("&#13;"),
            '\t' | '\n' => output.push(c),
            c if (c >= '\u{20}' && c <= '\u{d7ff}') || (c >= '\u{e000}' && c <= '\u{fffd}') || c >= '\u{10000}' => output.push(c),
            _ => {},
        }
    }
    output
}

fn column(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop { letters.push((b'A' + (index % 26) as u8) as char); if index < 26 { break; } index = index / 26 - 1; }
    letters.iter().rev().collect()
}

pub(super) fn workbook(csv: &str) -> Result<Vec<u8>, String> {
    let rows = parse_csv(csv)?;
    let headers = &rows[0];
    let dimension = format!("A1:{}{}", column(headers.len() - 1), rows.len());
    let mut sheet = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><dimension ref=\"{dimension}\"/><sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/><selection pane=\"bottomLeft\" activeCell=\"A2\" sqref=\"A2\"/></sheetView></sheetViews><sheetFormatPr defaultRowHeight=\"15\"/><cols>");
    for (i, header) in headers.iter().enumerate() {
        let width = if header.contains("memo") {50} else if header.contains("name") {32} else if header.ends_with("_cents") {20} else {18};
        sheet.push_str(&format!("<col min=\"{}\" max=\"{}\" width=\"{width}\" customWidth=\"1\"/>", i + 1, i + 1));
    }
    sheet.push_str("</cols><sheetData>");
    for (r, record) in rows.iter().enumerate() {
        sheet.push_str(&format!("<row r=\"{}\">", r + 1));
        for (c, value) in record.iter().enumerate() {
            if value.encode_utf16().count() > 32767 { return Err("a cell exceeds Excel's 32,767-character limit; download CSV instead".into()); }
            let reference = format!("{}{}", column(c), r + 1);
            let number = value.parse::<i64>().ok();
            let numeric = r > 0 && (headers[c].ends_with("_cents") || headers[c] == "entry_number") && number.is_some();
            if numeric {
                if number.is_some_and(|n| n.checked_abs().is_none_or(|n| n > 999_999_999_999_999)) { return Err("a number exceeds Excel's exact 15-digit precision; download CSV instead".into()); }
                sheet.push_str(&format!("<c r=\"{reference}\" s=\"2\" t=\"n\"><v>{value}</v></c>"));
            } else {
                sheet.push_str(&format!("<c r=\"{reference}\" s=\"{}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", if r == 0 {1} else {0}, xml(value)));
            }
        }
        sheet.push_str("</row>");
        if sheet.len() > MAX_WORKBOOK - 20000 { return Err("workbook exceeds the 10 MiB output limit".into()); }
    }
    sheet.push_str(&format!("</sheetData><autoFilter ref=\"{dimension}\"/></worksheet>"));
    let files = [
        ("[Content_Types].xml", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/></Types>".to_string()),
        ("_rels/.rels", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".to_string()),
        ("xl/workbook.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><bookViews><workbookView/></bookViews><sheets><sheet name=\"NXT Report\" sheetId=\"1\" r:id=\"rId1\"/></sheets><calcPr calcId=\"191029\"/></workbook>".to_string()),
        ("xl/_rels/workbook.xml.rels", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>".to_string()),
        ("xl/styles.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><color rgb=\"FFFFFFFF\"/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"3\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF086F68\"/><bgColor indexed=\"64\"/></patternFill></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"3\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"0\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\"/><xf numFmtId=\"3\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>".to_string()),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    zip(&files)
}

fn u16le(output: &mut Vec<u8>, value: u16) { output.extend_from_slice(&value.to_le_bytes()); }
fn u32le(output: &mut Vec<u8>, value: u32) { output.extend_from_slice(&value.to_le_bytes()); }
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 { crc = (crc >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(crc & 1))); }
    }
    !crc
}

fn zip(files: &[(&str, String)]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new(); let mut directory = Vec::new();
    for (name, contents) in files {
        if name.len() > u16::MAX as usize || contents.len() > MAX_WORKBOOK { return Err("invalid workbook ZIP entry size".into()); }
        let offset = output.len() as u32; let bytes = contents.as_bytes(); let length = bytes.len() as u32; let crc = crc32(bytes);
        u32le(&mut output, 0x04034b50); u16le(&mut output, 20); u16le(&mut output, 0x0800); u16le(&mut output, 0);
        u16le(&mut output, 0); u16le(&mut output, 0x0021); u32le(&mut output, crc); u32le(&mut output, length); u32le(&mut output, length);
        u16le(&mut output, name.len() as u16); u16le(&mut output, 0); output.extend_from_slice(name.as_bytes()); output.extend_from_slice(bytes);
        u32le(&mut directory, 0x02014b50); u16le(&mut directory, 20); u16le(&mut directory, 20); u16le(&mut directory, 0x0800); u16le(&mut directory, 0);
        u16le(&mut directory, 0); u16le(&mut directory, 0x0021); u32le(&mut directory, crc); u32le(&mut directory, length); u32le(&mut directory, length);
        u16le(&mut directory, name.len() as u16); u16le(&mut directory, 0); u16le(&mut directory, 0); u16le(&mut directory, 0); u16le(&mut directory, 0); u32le(&mut directory, 0); u32le(&mut directory, offset); directory.extend_from_slice(name.as_bytes());
        if output.len() + directory.len() + 22 > MAX_WORKBOOK { return Err("workbook exceeds the 10 MiB output limit".into()); }
    }
    let offset = output.len() as u32; let length = directory.len() as u32; output.extend_from_slice(&directory);
    u32le(&mut output, 0x06054b50); u16le(&mut output, 0); u16le(&mut output, 0); u16le(&mut output, files.len() as u16); u16le(&mut output, files.len() as u16); u32le(&mut output, length); u32le(&mut output, offset); u16le(&mut output, 0);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_preserves_quoted_delimiters_newlines_unicode_and_empty_fields() {
        let rows = parse_csv("memo,name,amount_cents\r\n\"Hello, \"\"team\"\"\r\nnext line\",Café,12345\r\n,,0\r\n").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["Hello, \"team\"\r\nnext line", "Café", "12345"]);
        assert_eq!(rows[2], vec!["", "", "0"]);
        assert!(parse_csv("name\r\n\"unterminated").is_err());
        assert!(parse_csv("name\r\n\"value\"suffix").is_err());
    }
    #[test]
    fn workbook_has_zip_directory_typed_numbers_and_no_formulas() {
        let bytes = workbook("account_code,account_name,debit_cents\r\n0010,\"=SUM(1,2) & <name>\",12345\r\n").unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert!(bytes.windows(4).any(|w| w == b"PK\x01\x02"));
        assert_eq!(&bytes[bytes.len()-22..bytes.len()-18], b"PK\x05\x06");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("<v>12345</v>"));
        assert!(text.contains("=SUM(1,2) &amp; &lt;name&gt;"));
        assert!(text.contains(">0010</t>"));
        assert!(text.contains("state=\"frozen\""));
        assert!(!text.contains("<f>"));
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
        assert!(workbook("debit_cents\r\n9223372036854775807\r\n").is_err());
        assert!(workbook(&format!("memo\r\n{}\r\n", "x".repeat(32768))).is_err());
    }
}
