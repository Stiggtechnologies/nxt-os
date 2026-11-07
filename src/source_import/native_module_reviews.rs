//! Immutable native-source origin reviews. Original T3 history and personal holds
//! remain unchanged; raw E11 wire facts are exposed only in this separate review.
use super::personal_source_info::PROFILE as PERSONAL_PROFILE;
use super::{ImportError, SourceImport, bad, conflict, missing, storage};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

type Result<T> = std::result::Result<T, ImportError>;
pub const PROFILE: &str = "d33fd86f7924d50e96f6ce4916ff14df8d4740a46863fb13cb1900842dd59604";
const LINES: [(&str, u64); 6] = [
    ("15000", 150),
    ("23600", 236),
    ("26000", 260),
    ("42000", 420),
    ("48400", 484),
    ("48500", 485),
];
// Client-free, full canonical payload digests. These pin all88 original source
// identities, native files, archive frames and raw field facts without putting
// client records, identifiers, names or amounts in source control.
const REVIEW_DIGESTS: [&str; 88] = [
    "09052da100c9a54eaa5d7fd85ddd61fa4d734b512d9b065be40d8576e388c8e9",
    "0f63023031397538b9c4bee8a1cb87a68ae2de4b6f8f881d6bde2b873623f5b6",
    "106e0cd2a0cecd69141e988e04cc143522ea6795e6048a0e3abf2db264298288",
    "1073549823d507afaefa474ce7aee80c7f04651fa237f015ee794064896f7edd",
    "10b8e216d328cfc9c84dd7a35c638864ad009fd33b96ed5cac5e666d50b849d1",
    "10f0cfc665bee19aa541489934562f21831023e395555b6bf7924f627390e995",
    "1132000bfa65a3098219118d9cb2c675d18b27a44bda22035d70382f45f26fee",
    "137083c0f9f3e5a9c41c4c06bb8c7a4a97400bd6f76cfa41b67e31c1005781a5",
    "142d69309f92124a118a746d1e02793c5ed6cd098852a45b8b80e4403c8f2a0a",
    "14668c3adc6b9ec95c152b3db673bf6926b2593a680bdf694468440ff7755b6f",
    "14ffbccfe14b5a214ce6f9d4ec4c08ba8832fdb570d21e3f5f02c2a56ae157a8",
    "1a6f979159700618fedcbf5b0a07ef6434354945fd60763759d63df80e0f3c4c",
    "1d6251458272c531b97d2103b97a525e3a923b9e393e4e9362cec29bdaba242f",
    "1eba860da8322224cab35e65cf22f496b8cba62e71f0ea78df7c59fcf07e4e99",
    "21b80c3fa246b7cf9c1558cc658a9865c861573016dffd91799fa4541e90af4c",
    "2359fc1bc3ab3d7a0d6494bc7733232b34184407be30d9ba59355d153c9ef5fb",
    "30e6ffb7c326d67ce28d714b6b3363265a5181e31c078cd7a27f2f47f29c0e4c",
    "3307523f49745addb2f443e40f7829ade898f33a3d5657b820c0f22cc1b07f4f",
    "432f4eb08a4e6010c9d90b451239d650a2ddb2b4348b15dd252e898a3118953b",
    "444e5cc66236d1cd23c2d62421980a2e575456e45c51adfb4939659e24157f1c",
    "4559d9dd63228e4dc9fd13b0d7bfa269011667cb4e7c8e8ee063cde120f0286a",
    "471e529e1b352af30e71714294bd2ed614bc78efafeacbd7c7e174479027b1b7",
    "473bbf7f54f1bd9c18bf2bc25564a05cd82dcbd8f794786bcf104076a22d692f",
    "4ae6321f7f3862fa965d957825bcb0f85c5a501c80e84763be4c8f6abbda7eba",
    "4ded50bc47885380197d214c77855d48868349b85bcaa94028ce18e7b233c184",
    "4f25a87f92f8e7518f56cd950e70b67bf50447a43e78f2134a90f2088cddbf3e",
    "4ff2ed159a82908949d254fde97750c81ec2f160ec9902daeb6a4f05e305956b",
    "5844408e9a316d091744349ae51295942a2efbc38b79967aebba54cab27ea199",
    "598cafb85da7beab001a82e1927f44edc08fd3da53403b46cb80bcbc6a66f9ef",
    "5b0ae2cee3c7a1b7bf7ce36b47540dc63a073e9fe07b46c91e57a30312340ebd",
    "5f9a6b31dc8632429aa6a18f3a0154f4f40640fac1a160677b367ad090b85f3e",
    "5f9d37613831d8c3282601747f928c5267e6c4f1c9e9c548c416907387dd7ceb",
    "611e155cc17c9a4de7c0594d6984fda52b5647c68138b591012dcc0702be8ae4",
    "626beef794b3cd219db7dcaeddacb6c604aed2e2a43dc54919938229ab1530c6",
    "63dc7db87f794f990fc23e796f151f746b07ae7c3a25b67b7109878fb5ec4716",
    "69721d27bc1dc3939bcb2ae87605489b8c7e02c7d5c2825195b2fcb68a6590e8",
    "6d71245abc1310424bfe3bf670f9b54a4eb5804803e7ae7861c089fb7829a35d",
    "6e511ded00abefed4b63ee3c15945f76442db93c23fb16dc71d7ee380d3d3cd7",
    "707778f540f1f6d362fef31abbdaba6abecd757a10563ad69d81e4c154d1dd3b",
    "79559adb8d39a22b49f2687b4b3f94bc494ff2b336fa68d61b1925d6cfd4d8b0",
    "7a136a56516725cd1f3d61007497e9d70c89e9e454ace515e4065f184c9ac2a2",
    "7b06bf0119c3967147147877b955daa1c7004f25ac5df421a3b99714edb7cbd9",
    "7daf352e149cd600fb38b61cde71785749b6512b1e7df7fc3d5d3d82ec212c1b",
    "864a8e2b9dd1c1b9963b413d88c279875d85432233dbd54d96df6ecb9d74db5c",
    "8824a97e336777ccbd0eb36679241b58fb1a0fe72a998a5643b216d68b9c3d68",
    "88faaaa74b3617b1ec780010dc074b6f97446a670dc5072dee95e33953812fec",
    "8d921bcdf97d4d715d742ceb99c5aeb46ce06891d15281d7c18ac82d8287ab6d",
    "930ac7c9f3af05fff1c7faf3c2534addc202477d6dc67a6c17f38ab88b53c564",
    "96fc6323b60f84b595d9ff07aee64fdd3d1b89bd2f7ffa12160bd9c0fe31065a",
    "9a91d66f315ddf7f16c9d35334f29691efb8bffdd52f5f2ab878b294fb2dfd9d",
    "9bcafb89ea330611b0fd84f3e3e01cc64ae135bad77eaaf09f440770fc3cd1ee",
    "9bfdc4a395109111330f6f5cd45176647e19d8d28aa59a9595852ec4a74ae3ff",
    "9d75ee6fa8369002b64ad4fc1467556756dd42117b6546516abcd8fc32b92df5",
    "a6156049915f1643fb2289e832a9cdb6c43550b00cc1be577091fb8df30f6c6b",
    "a7d046b7600562d3a5201d37f7e288f30c03d9dc4a731cd32cc16a9b84f97f21",
    "a7f64fbda9f22799817c078b118b9637665512f6e383e28210a72bc58b9d73c9",
    "a9f30a3daf068b9417964ebc3104b8dce975a8e2bb4acc26dc87bcbb2f678fa8",
    "ac21a9d0b78c7621784a543877cf3584f48334fbdca99bc7ac93b85df0e1e8fb",
    "ac43e7293781418a852de04df3749889e5eab8476bfee0a950184f2b8bd410d1",
    "ae4b2b0000536310e7056d4fc73c7406baea118cb9921a2e7ce5480499cebf94",
    "af9aaa5df41c93ce703209485d72e2061e097c907709b3d82d687ebe6a41c262",
    "b003002e144909d45fe6e599cf66aa0e8268f7e3e8cbbebc4775ea0ec2969bba",
    "b2bbc215dc8e3d7bbb75db51c6092f49ccffdec2bcefb88525d8e56bd79ed67c",
    "b741aee9f773fdc0340c1383fa23eeace52d3e8c26378bcdeb83b963841da2de",
    "b7a9d93dea2d2306e1a52e8c546fd9707038636122f3de16bb1051416cf29396",
    "c1329e83d3de4d5649c6d72b7b08aa55a4f93cedbfa85f582d09a8406f6c44e5",
    "c3fbc7b7d3a94c0cb59f3322135cf77d5d273f0f355120216c2d8b6a0f4aa49f",
    "c54347b55df57cdfcb599849f71f62c610bf4ef732f55218f067f3077dbaf4be",
    "cc0f6344f37b3dd60ef0e11733fe479be267c1b08fff0e724299b13462d4eb6a",
    "d1079fbad1c0c0fd8a2456adfaac3d399b267e7be1c86b7a43c8e8ff710f3482",
    "d3cb08caddd364d2a0ed092ae62bbddfdebab20d408e8b1f262a62b298c211fe",
    "d5191761068971c123e2f4f8480a9ceb22bdbb8165f48c740d7940d639885032",
    "d639cb8ddc280d47c81c6dd53ea027842f1a81965febdb4519fe607764fe3030",
    "db155dacb439c0d59a6a97fcdb14c14e8a47e524b489738daeaa56ce302d1e2a",
    "dc5d563ba42329f6925512cfbb8e4dd1fcbdced83e70257b56de84d675a8c101",
    "dd70b7d2ab29a13a998512dc2dd190aadbfbd9d08c0907da11f53bb6481e23be",
    "df4f60c5104b373675e3b8b43de5c8c88228e0641620f0b5e86d455492f4ef27",
    "e2b7b9c2457a4be94f2dd974d804a8ddc9371455e91ec3267f67900e42d69c07",
    "e55d8d80687f33c8d6f8cfa574154ae5df3e77ee2561e73edd7243db59dc955f",
    "e5bbd6e534dd7327ea62363158de33d80cffac44700ca6f9b994e525bc77bace",
    "e9e3a3d77a78cd8291e886d0f116b78f80d713d2ab7f00128bcae80b347962c3",
    "f039305933dccdf2205d27ede607ec06713381b9c5fc7833a3c07ca8992a16c8",
    "f2b5cce3b1a6b8e70ab027366d11f6819e07b9b9346acaf645d982ca952398bf",
    "f38b9a7c5655f4b11eb62c246a143deb40133785d6bcca5d3491a0c895696091",
    "f489d64cee9f9a9e512f772a24823154d111f51a66b6a0d33994b2a31b35586a",
    "f7192c0a7c250b15c841fa65d8e193e3d3e90471e17a2953a427984e4676e939",
    "f7eebcad49efcb2a2fbd92d4c7aad87307ba93a0ac050d9d911c4e5a62caf130",
    "fff2a17d781b073591ba6dab456f5f342049ef7e532018c268f476b9d1a43479",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModuleReviewBatch {
    pub test_copy: bool,
    pub binding_profile_sha256: String,
    pub records: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModuleReviewLookup {
    pub test_copy: bool,
    pub record_ids: Vec<String>,
}

fn profile() -> &'static Value {
    static P: OnceLock<Value> = OnceLock::new();
    P.get_or_init(|| {
        serde_json::from_str(include_str!("native_module_review_profile.json"))
            .expect("pinned native module bindings")
    })
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn number(s: &str) -> bool {
    s.parse::<u32>()
        .is_ok_and(|n| n > 0 && n <= i32::MAX as u32 && n.to_string() == s)
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| bad("Native source review string missing"))
}
fn identity(r: &Value) -> Result<String> {
    Ok(format!(
        "{}:{}:{}",
        text(r, "client_number")?,
        r["source_year"]
            .as_u64()
            .ok_or_else(|| bad("Source year missing"))?,
        text(r, "archive_sha256")?
    ))
}
fn valid_identity(id: &str) -> bool {
    let parts: Vec<_> = id.split(':').collect();
    parts.len() == 3
        && number(parts[0])
        && parts[1]
            .parse::<u16>()
            .is_ok_and(|y| (1990..=2100).contains(&y) && y.to_string() == parts[1])
        && hash(parts[2])
}
fn key(c: &str, id: &str) -> String {
    format!("dtmaxtc:{c}:native_module_review:{PROFILE}:record:{id}")
}
fn index(c: &str) -> String {
    format!("dtmaxtc:{c}:native_module_review:{PROFILE}:all")
}
fn history_key(c: &str, id: &str) -> String {
    format!("dtmaxtc:{c}:historical_lines:{id}")
}
fn personal_key(c: &str, id: &str) -> String {
    format!("dtmaxtc:{c}:personal_source:{PERSONAL_PROFILE}:record:{id}")
}

// Explicit lexical-key canonicalization, independent of serde_json's map feature.
// Frozen review payloads contain only ASCII noncontrol strings and integer JSON
// numbers. No float, unmasked personal field or caller-chosen extension is accepted.
fn canonical(v: &Value) -> Result<Vec<u8>> {
    fn write(v: &Value, out: &mut Vec<u8>, depth: usize) -> Result<()> {
        if depth > 24 || out.len() > 16_384 {
            return Err(bad("Native review payload exceeds bound"));
        }
        match v {
            Value::Object(m) => {
                out.push(b'{');
                let sorted: BTreeMap<_, _> = m.iter().collect();
                for (i, (k, v)) in sorted.into_iter().enumerate() {
                    if i > 0 {
                        out.push(b',')
                    }
                    write(&Value::String(k.clone()), out, depth + 1)?;
                    out.push(b':');
                    write(v, out, depth + 1)?;
                }
                out.push(b'}');
            }
            Value::Array(a) => {
                out.push(b'[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(b',')
                    }
                    write(v, out, depth + 1)?
                }
                out.push(b']');
            }
            Value::String(s) => {
                if !s.is_ascii() || s.bytes().any(|b| b.is_ascii_control()) {
                    return Err(bad("Unsupported native review text"));
                }
                serde_json::to_writer(&mut *out, s)
                    .map_err(|_| bad("Native review text encoding failed"))?;
            }
            Value::Number(n) => {
                if !n.is_i64() && !n.is_u64() {
                    return Err(bad("Floating native source value refused"));
                }
                out.extend_from_slice(n.to_string().as_bytes());
            }
            Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
            Value::Null => out.extend_from_slice(b"null"),
        }
        if out.len() > 16_384 {
            return Err(bad("Native review payload exceeds bound"));
        }
        Ok(())
    }
    let mut bytes = Vec::new();
    write(v, &mut bytes, 0)?;
    Ok(bytes)
}
fn pinned_bytes(r: &Value) -> Result<Vec<u8>> {
    let raw = canonical(r)?;
    let digest = hex::encode(Sha256::digest(&raw));
    if REVIEW_DIGESTS.binary_search(&digest.as_str()).is_err() {
        return Err(bad(
            "Native source review is outside the88 verified payload pins",
        ));
    }
    Ok(raw)
}
fn wire_decimal(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let n = cents.abs();
    let whole = n / 100;
    let fraction = n % 100;
    if fraction == 0 {
        format!("{sign}{whole}")
    } else if fraction % 10 == 0 {
        format!("{sign}{whole}.{}", fraction / 10)
    } else {
        format!("{sign}{whole}.{fraction:02}")
    }
}
fn validate(r: &Value, context: &Value, personal: &Value) -> Result<Vec<u8>> {
    let raw = pinned_bytes(r)?;
    let id = identity(r)?;
    if !valid_identity(&id)
        || !number(text(r, "family_file_id")?)
        || r["schema"] != "dtmax-native-source-module-review/v1"
        || r["binding_profile_sha256"] != PROFILE
        || r["reader_module"] != "T3"
        || r["native_source_module"] != "T1"
        || r["review_state"] != "native_source_module_origin_verified"
    {
        return Err(bad("Native source review identity or profile differs"));
    }
    for flag in [
        "original_history_updated",
        "personal_sidecar_flags_updated",
        "identity_verified",
        "tax_figures_approved",
    ] {
        if r[flag] != false {
            return Err(bad(
                "Native origin is not an identity, figure or profile approval",
            ));
        }
    }
    if context["client_number"] != r["client_number"]
        || context["family_file_id"] != r["family_file_id"]
        || context["tax_year"] != r["source_year"]
        || context["archive_sha256"] != r["archive_sha256"]
        || context["vendor_module"] != "T3"
        || context["mapping_state"] != "pending_module_verification"
    {
        return Err(bad(
            "Native source review requires exact original held historical context",
        ));
    }
    let original_values = context["values_cents"]
        .as_object()
        .ok_or_else(|| bad("Original six-field presence map missing"))?;
    if original_values.len() != 6
        || LINES
            .iter()
            .any(|(line, _)| !original_values.contains_key(*line))
    {
        return Err(bad("Original six-field presence map differs"));
    }
    let frames = &r["source_frames"];
    if personal["client_number"] != r["client_number"]
        || personal["family_file_id"] != r["family_file_id"]
        || personal["source_year"] != r["source_year"]
        || personal["archive_sha256"] != r["archive_sha256"]
        || personal["reader_module"] != "T3"
        || personal["display_state"] != "held_module_origin"
        || personal["t1_display_eligible"] != false
        || personal["info_record_state"] != "present_nonempty_crc_verified"
        || personal["info1"] != r["info1"]
        || personal["source_EXF_file_id"] != frames["EXF"]["selected_file_id"]
        || personal["source_year_frame_offset"] != frames["EXY"]["frame_offset"]
        || personal["source_context_offset"] != frames["EXC"]["frame_offset"]
        || personal["source_EXC_sha256"] != frames["EXC"]["frame_sha256"]
    {
        return Err(bad(
            "Native origin differs from immutable personal Info1 provenance",
        ));
    }
    let fields = personal["fields"]
        .as_object()
        .ok_or_else(|| bad("Original personal source fields missing"))?;
    if fields.len() != 7
        || fields.values().any(|f| f["display_eligible"] != false)
        || personal["fields"]["SIN"]["state"] != "source_placeholder_zero_sentinel"
        || !personal["fields"]["SIN"]["candidate"].is_null()
        || personal["fields"]["BirthDate"]["state"] != "present_blank"
        || !personal["fields"]["BirthDate"]["candidate"].is_null()
    {
        return Err(bad(
            "Original personal identity holds must remain unchanged",
        ));
    }
    if frames["EXC"]["client_number"] != r["client_number"]
        || frames["EXY"]["native_tax_year"] != r["source_year"]
        || frames["EXF"]["selected_file_id"] != frames["EXC"]["member_file_id"]
        || frames["EXC"]["descriptor_terminal_byte"] != 0
        || r["info1"]["crc_verified"] != true
    {
        return Err(bad("Native archive frame identity differs"));
    }
    let family = text(r, "family_file_id")?
        .parse::<i64>()
        .map_err(|_| bad("Native file identity missing"))?;
    if frames["EXF"]["selected_file_id"].as_i64() != Some(family) {
        return Err(bad("Native selected file differs from family identity"));
    }
    let year = r["source_year"]
        .as_u64()
        .ok_or_else(|| bad("Native year missing"))?;
    if r["native_file"]["relative_path"] != format!("T1\\{year}\\{family}.P{:02}", year % 100)
        || r["native_file"]["physical_raw_stream"] != true
        || r["native_authority"]["module_index"] != 1
        || r["native_authority"]["path_rule_exact"] != true
        || r["native_authority"]["source_archive_magic"] != "0xaeaeaebe"
    {
        return Err(bad("Native module1 storage path or signature differs"));
    }
    let p = profile();
    for (key, path) in [
        (
            "path_formatter_body_sha256",
            &p["native_functions"]["path_formatter"]["sha256"],
        ),
        ("extension_table_sha256", &p["extension_table"]["sha256"]),
        (
            "init_body_sha256",
            &p["native_functions"]["flat_initializer"]["sha256"],
        ),
    ] {
        if r["native_authority"][key] != *path {
            return Err(bad("Native byte authority differs"));
        }
    }
    let e11 = &r["e11"];
    if e11["source_personal_prefix"] != 182 {
        return Err(bad("Personal calculated record prefix differs"));
    }
    let efields = e11["fields"]
        .as_object()
        .ok_or_else(|| bad("Raw E11 field set missing"))?;
    if efields.len() != 6 {
        return Err(bad("All six raw E11 presence states are required"));
    }
    for (line, fid) in LINES {
        let f = &e11["fields"][line];
        if f["field_id"] != fid {
            return Err(bad("Raw E11 field identifier differs"));
        }
        match f["state"].as_str() {
            Some("absent") => {
                if !context["values_cents"][line].is_null()
                    || [
                        "source_decimal",
                        "source_wire_integer",
                        "scale_divisor",
                        "field_start",
                        "field_end",
                        "source_field_bytes_sha256",
                    ]
                    .iter()
                    .any(|k| !f[*k].is_null())
                {
                    return Err(bad("Absent raw E11 field cannot become zero or a value"));
                }
            }
            Some("explicit_zero" | "present_value") => {
                let wire = f["source_wire_integer"]
                    .as_i64()
                    .filter(|n| i32::try_from(*n).is_ok())
                    .ok_or_else(|| bad("Raw E11 wire integer outside source bound"))?;
                let form = &e11["form"];
                if form["name"] != "E11"
                    || form["record_type"] != 30
                    || form["outer_plan"] != 0
                    || form["crc_verified"] != true
                    || form["marker_field1_present"] != true
                    || f["scale_divisor"] != 100
                    || f["source_decimal"] != wire_decimal(wire)
                    || context["values_cents"][line] != wire.to_string()
                    || (f["state"] == "explicit_zero") != (wire == 0)
                {
                    return Err(bad(
                        "Raw T1 E11 source differs from preserved field presence or exact cents",
                    ));
                }
            }
            _ => return Err(bad("Unsupported raw E11 field state")),
        }
    }
    if e11["form"].is_null() {
        if e11["reader_has_calculated_return"] != false {
            return Err(bad("Source calculated-record flag differs"));
        }
    } else if e11["reader_has_calculated_return"] != true {
        return Err(bad("Source E11 requires its recorded calculated segment"));
    }
    Ok(raw)
}
fn parse_stored(raw: Option<String>, what: &str) -> Result<Value> {
    serde_json::from_str(&raw.ok_or_else(|| missing(what))?)
        .map_err(|_| storage("Stored native review dependency unreadable"))
}

impl SourceImport {
    pub async fn test_copy_native_module_reviews(
        &self,
        company: &str,
        batch: NativeModuleReviewBatch,
    ) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.binding_profile_sha256 != PROFILE
            || batch.records.is_empty()
            || batch.records.len() > 250
        {
            return Err(bad("Native module review profile or batch bound differs"));
        }
        let mut ids = Vec::new();
        let mut seen = BTreeSet::new();
        for r in &batch.records {
            pinned_bytes(r)?;
            let id = identity(r)?;
            if !valid_identity(&id) || !seen.insert(id.clone()) {
                return Err(bad("Duplicate or malformed native review identity"));
            }
            ids.push(id)
        }
        let mut dependency_keys: Vec<_> = ids.iter().map(|id| history_key(company, id)).collect();
        dependency_keys.extend(ids.iter().map(|id| personal_key(company, id)));
        let dependencies = self
            .store
            .mget(&dependency_keys)
            .await
            .map_err(|e| storage(&e))?;
        if dependencies.len() != 2 * ids.len() {
            return Err(storage("Native review dependency cardinality differs"));
        }
        let mut prepared = Vec::new();
        for (i, r) in batch.records.iter().enumerate() {
            let context = parse_stored(
                dependencies[i].clone(),
                "Original historical context missing",
            )?;
            let personal = parse_stored(
                dependencies[ids.len() + i].clone(),
                "Original personal source missing",
            )?;
            let raw = String::from_utf8(validate(r, &context, &personal)?)
                .map_err(|_| storage("Native review encoding failed"))?;
            prepared.push((key(company, &ids[i]), raw, ids[i].clone()))
        }
        let keys: Vec<_> = prepared.iter().map(|x| x.0.clone()).collect();
        let existing = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if existing.len() != prepared.len() {
            return Err(storage("Native review preflight cardinality differs"));
        }
        for ((_, raw, _), old) in prepared.iter().zip(existing) {
            if old.as_ref().is_some_and(|v| v != raw) {
                return Err(conflict("Native source review is immutable"));
            }
        }
        let (mut created, mut unchanged) = (0, 0);
        for chunk in prepared.chunks(4) {
            let mut tasks = tokio::task::JoinSet::new();
            for (k, raw, id) in chunk {
                let (store, k, raw, id, all) = (
                    self.store.clone(),
                    k.clone(),
                    raw.clone(),
                    id.clone(),
                    index(company),
                );
                tasks.spawn(async move {
                    let added = store.set_nx(&k, &raw).await.map_err(|e| storage(&e))?;
                    if store.get(&k).await.map_err(|e| storage(&e))?.as_deref() != Some(&raw) {
                        return Err(conflict("Native source review is immutable"));
                    }
                    store.sadd(&all, &id).await.map_err(|e| storage(&e))?;
                    Ok(added)
                });
            }
            while let Some(result) = tasks.join_next().await {
                if result.map_err(|_| storage("Native review write interrupted"))?? {
                    created += 1
                } else {
                    unchanged += 1
                }
            }
        }
        Ok(
            json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":unchanged,"original_historical_records_modified":0,"personal_source_flags_modified":0,"current_profiles_modified":0,"tax_figures_approved":0}),
        )
    }
    pub(super) async fn attach_native_module_reviews(
        &self,
        company: &str,
        records: &mut [Value],
    ) -> Result<()> {
        if records.len() > 250 {
            return Err(bad("Native review page exceeds bound"));
        }
        if records.is_empty() {
            return Ok(());
        }
        let ids: Vec<_> = records
            .iter()
            .map(|r| {
                format!(
                    "{}:{}:{}",
                    r["client_number"].as_str().unwrap_or(""),
                    r["tax_year"],
                    r["archive_sha256"].as_str().unwrap_or("")
                )
            })
            .collect();
        let mut keys: Vec<_> = ids.iter().map(|id| key(company, id)).collect();
        keys.extend(ids.iter().map(|id| personal_key(company, id)));
        let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if rows.len() != 2 * records.len() {
            return Err(storage("Native review page cardinality differs"));
        }
        for (i, context) in records.iter_mut().enumerate() {
            if let Some(raw) = &rows[i] {
                let review: Value =
                    serde_json::from_str(raw).map_err(|_| storage("Native review unreadable"))?;
                let personal = parse_stored(
                    rows[ids.len() + i].clone(),
                    "Original personal source missing",
                )?;
                validate(&review, context, &personal)
                    .map_err(|_| storage("Stored native review provenance differs"))?;
                context["native_module_review"] = review
            }
        }
        Ok(())
    }
    pub async fn test_copy_read_native_module_reviews(
        &self,
        company: &str,
        lookup: NativeModuleReviewLookup,
    ) -> Result<Value> {
        self.test_copy_guard(company, lookup.test_copy)?;
        if lookup.record_ids.is_empty()
            || lookup.record_ids.len() > 250
            || lookup.record_ids.iter().any(|id| !valid_identity(id))
        {
            return Err(bad("Native review lookup bound or identity differs"));
        }
        let n = lookup.record_ids.len();
        let mut keys: Vec<_> = lookup
            .record_ids
            .iter()
            .map(|id| key(company, id))
            .collect();
        keys.extend(lookup.record_ids.iter().map(|id| history_key(company, id)));
        keys.extend(lookup.record_ids.iter().map(|id| personal_key(company, id)));
        let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if rows.len() != 3 * n {
            return Err(storage("Native review lookup cardinality differs"));
        }
        let mut records = Vec::new();
        for (i, id) in lookup.record_ids.iter().enumerate() {
            let review = parse_stored(rows[i].clone(), "Native source review not imported")?;
            let context = parse_stored(rows[n + i].clone(), "Original historical context missing")?;
            let personal =
                parse_stored(rows[2 * n + i].clone(), "Original personal source missing")?;
            validate(&review, &context, &personal)
                .map_err(|_| storage("Stored native review provenance differs"))?;
            if identity(&review)? != *id {
                return Err(storage("Native review key differs"));
            }
            records.push(json!({"native_module_review":review,"original_historical_record":context,"personal_source_flags_modified":false}))
        }
        Ok(
            json!({"test_copy":true,"source_scope":company,"binding_profile_sha256":PROFILE,"record_ids":lookup.record_ids,"records":records,"current_profiles_modified":0,"original_historical_records_modified":0,"personal_source_flags_modified":0}),
        )
    }
    pub async fn test_copy_native_module_review_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let ids = self
            .store
            .smembers(&index(company))
            .await
            .map_err(|e| storage(&e))?;
        if ids.len() > 88 {
            return Err(storage("Native review index exceeds pinned cohort"));
        }
        let keys: Vec<_> = ids.iter().map(|id| key(company, id)).collect();
        let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if rows.len() != ids.len() {
            return Err(storage("Native review summary cardinality differs"));
        }
        let (mut forms, mut present, mut zeros, mut absent) = (0, 0, 0, 0);
        for (id, raw) in ids.iter().zip(rows) {
            let r = parse_stored(raw, "Native review index missing record")?;
            pinned_bytes(&r).map_err(|_| storage("Native review summary pin differs"))?;
            if identity(&r)? != *id {
                return Err(storage("Native review index identity differs"));
            }
            forms += usize::from(!r["e11"]["form"].is_null());
            for (line, _) in LINES {
                match r["e11"]["fields"][line]["state"].as_str() {
                    Some("absent") => absent += 1,
                    Some("explicit_zero") => {
                        present += 1;
                        zeros += 1
                    }
                    Some("present_value") => present += 1,
                    _ => return Err(storage("Native review field state differs")),
                }
            }
        }
        Ok(
            json!({"test_copy":true,"source_scope":company,"binding_profile_sha256":PROFILE,"module_origin_reviews":ids.len(),"native_source_module":"T1","original_reader_module":"T3","raw_E11_forms":forms,"raw_E11_present_fields":present,"raw_E11_explicit_zero_fields":zeros,"raw_E11_absent_fields":absent,"current_profiles_modified":0,"original_historical_records_modified":0,"personal_source_flags_modified":0,"tax_figures_approved":0}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    #[test]
    fn profile_and_client_free_allowlist_are_frozen() {
        assert_eq!(
            hex::encode(Sha256::digest(include_bytes!(
                "native_module_review_profile.json"
            ))),
            PROFILE
        );
        assert_eq!(REVIEW_DIGESTS.len(), 88);
        assert!(REVIEW_DIGESTS.windows(2).all(|w| w[0] < w[1]));
        assert!(REVIEW_DIGESTS.iter().all(|s| hash(s)));
        assert_eq!(profile()["module1_rule"]["module_index"], 1);
        assert_eq!(profile()["extension_table"]["index_characters"]["1"], "P");
        for (n, text) in [
            (0, "0"),
            (1, "0.01"),
            (10, "0.1"),
            (100, "1"),
            (-1, "-0.01"),
            (i32::MIN as i64, "-21474836.48"),
        ] {
            assert_eq!(wire_decimal(n), text)
        }
    }
    #[tokio::test]
    async fn rejects_unpinned_payloads_before_any_write() {
        let service = SourceImport::new(Store::memory());
        let batch = NativeModuleReviewBatch {
            test_copy: true,
            binding_profile_sha256: PROFILE.into(),
            records: vec![
                json!({"reader_module":"T3","native_source_module":"T1","tax_figures_approved":false}),
            ],
        };
        assert!(
            service
                .test_copy_native_module_reviews("native-review-reject", batch)
                .await
                .is_err()
        );
        assert_eq!(
            service
                .test_copy_native_module_review_summary("native-review-reject")
                .await
                .unwrap()["module_origin_reviews"],
            0
        );
        assert!(canonical(&json!({"wire":0.5})).is_err());
        assert!(canonical(&json!({"text":"\n"})).is_err());
    }
    #[tokio::test]
    async fn optional_all88_private_reviews_are_exact_immutable_and_preserve_sources() {
        let Ok(path) = std::env::var("NXT_DTMAX_NATIVE_MODULE_REVIEWS_INPUT") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            "4105719748b0ed5414e407c827395b71e00ba76a082334a24cc23b32586c1d5e"
        );
        let reviews: Vec<Value> = std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(reviews.len(), 88);
        let store = Store::memory();
        let service = SourceImport::new(store.clone());
        let company = "private-native-module-review";
        let mut originals = Vec::new();
        for r in &reviews {
            let id = identity(r).unwrap();
            let mut cents = json!({});
            for (line, _) in LINES {
                cents[line] = r["e11"]["fields"][line]["source_wire_integer"]
                    .as_i64()
                    .map(|v| json!(v.to_string()))
                    .unwrap_or(Value::Null)
            }
            let context = json!({"client_number":r["client_number"],"family_file_id":r["family_file_id"],"tax_year":r["source_year"],"archive_sha256":r["archive_sha256"],"vendor_module":"T3","mapping_state":"pending_module_verification","values_cents":cents});
            let mut fields = json!({});
            for name in [
                "Relation",
                "SIN",
                "BirthDate",
                "Status",
                "Sex",
                "Prov-Residence",
                "Language",
            ] {
                fields[name] = json!({"display_eligible":false,"candidate":null,"state":"held"})
            }
            fields["SIN"]["state"] = json!("source_placeholder_zero_sentinel");
            fields["BirthDate"]["state"] = json!("present_blank");
            let personal = json!({"client_number":r["client_number"],"family_file_id":r["family_file_id"],"source_year":r["source_year"],"archive_sha256":r["archive_sha256"],"reader_module":"T3","display_state":"held_module_origin","t1_display_eligible":false,"info_record_state":"present_nonempty_crc_verified","info1":r["info1"],"source_EXF_file_id":r["source_frames"]["EXF"]["selected_file_id"],"source_year_frame_offset":r["source_frames"]["EXY"]["frame_offset"],"source_context_offset":r["source_frames"]["EXC"]["frame_offset"],"source_EXC_sha256":r["source_frames"]["EXC"]["frame_sha256"],"fields":fields});
            let h = serde_json::to_string(&context).unwrap();
            let p = serde_json::to_string(&personal).unwrap();
            store.set(&history_key(company, &id), &h).await.unwrap();
            store.set(&personal_key(company, &id), &p).await.unwrap();
            originals.push((id, h, p));
        }
        let sentinel = format!("dtmaxtc:{company}:client:current-sentinel");
        store.set(&sentinel, "unchanged baseline").await.unwrap();
        for replay in [false, true] {
            let result = service
                .test_copy_native_module_reviews(
                    company,
                    NativeModuleReviewBatch {
                        test_copy: true,
                        binding_profile_sha256: PROFILE.into(),
                        records: reviews.clone(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                result[if replay {
                    "unchanged_replays"
                } else {
                    "created"
                }],
                88
            )
        }
        let ids = originals.iter().map(|x| x.0.clone()).collect();
        let result = service
            .test_copy_read_native_module_reviews(
                company,
                NativeModuleReviewLookup {
                    test_copy: true,
                    record_ids: ids,
                },
            )
            .await
            .unwrap();
        for (stored, expected) in result["records"].as_array().unwrap().iter().zip(&reviews) {
            assert_eq!(&stored["native_module_review"], expected)
        }
        let mut contexts = Vec::new();
        for (id, h, p) in &originals {
            assert_eq!(
                store.get(&history_key(company, id)).await.unwrap().unwrap(),
                *h
            );
            assert_eq!(
                store
                    .get(&personal_key(company, id))
                    .await
                    .unwrap()
                    .unwrap(),
                *p
            );
            contexts.push(serde_json::from_str(h).unwrap());
        }
        service
            .attach_native_module_reviews(company, &mut contexts)
            .await
            .unwrap();
        for (context, r) in contexts.iter().zip(&reviews) {
            assert_eq!(&context["native_module_review"], r);
            assert_eq!(context["vendor_module"], "T3");
            assert_eq!(context["mapping_state"], "pending_module_verification");
        }
        let (_, h, p) = &originals[0];
        let original: Value = serde_json::from_str(h).unwrap();
        let personal: Value = serde_json::from_str(p).unwrap();
        let mut wrong = original.clone();
        wrong["family_file_id"] = json!("1");
        assert!(validate(&reviews[0], &wrong, &personal).is_err());
        let mut wrong = original.clone();
        wrong["values_cents"]["15000"] = json!("999999");
        assert!(validate(&reviews[0], &wrong, &personal).is_err());
        let mut wrong = personal.clone();
        wrong["t1_display_eligible"] = json!(true);
        assert!(validate(&reviews[0], &original, &wrong).is_err());
        assert_eq!(
            store.get(&sentinel).await.unwrap().unwrap(),
            "unchanged baseline"
        );
        let summary = service
            .test_copy_native_module_review_summary(company)
            .await
            .unwrap();
        assert_eq!(summary["module_origin_reviews"], 88);
        assert_eq!(summary["raw_E11_present_fields"], 59);
        assert_eq!(summary["raw_E11_explicit_zero_fields"], 55);
        assert_eq!(summary["raw_E11_absent_fields"], 469);
        let mut forged = reviews[0].clone();
        forged["tax_figures_approved"] = json!(true);
        assert!(
            service
                .test_copy_native_module_reviews(
                    company,
                    NativeModuleReviewBatch {
                        test_copy: true,
                        binding_profile_sha256: PROFILE.into(),
                        records: vec![forged]
                    }
                )
                .await
                .is_err()
        );
    }
}
