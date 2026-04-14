use ed25519_dalek::VerifyingKey;
use plist::Value;
use std::collections::BTreeMap;

/// Build the /info response as a binary plist.
///
/// This tells the iPhone what our receiver supports: mirroring, audio,
/// screen dimensions, protocol version, etc.
pub fn build_info_response(device_id: &str, public_key: &VerifyingKey) -> Vec<u8> {
    let pk_bytes = public_key.to_bytes();

    let mut dict = BTreeMap::new();

    // Device identification
    dict.insert(
        "deviceID".to_owned(),
        Value::String(device_id.to_owned()),
    );
    dict.insert("model".to_owned(), Value::String("AppleTV3,2".to_owned()));
    dict.insert(
        "name".to_owned(),
        Value::String("ios-remote".to_owned()),
    );

    // Protocol version
    dict.insert(
        "sourceVersion".to_owned(),
        Value::String("220.68".to_owned()),
    );
    dict.insert("vv".to_owned(), Value::Integer(2.into()));
    dict.insert(
        "protocolVersion".to_owned(),
        Value::String("1.1".to_owned()),
    );

    // Features bitmask: mirroring + screen + audio + photo
    // 0x5A7FFFF7 = most capabilities enabled
    dict.insert(
        "features".to_owned(),
        Value::Integer(0x5A7FFFF7_i64.into()),
    );

    // Status flags
    dict.insert("statusFlags".to_owned(), Value::Integer(0x44_i64.into()));

    // Public key for pairing (Ed25519)
    dict.insert("pk".to_owned(), Value::Data(pk_bytes.to_vec()));

    // Audio formats
    dict.insert("audioFormats".to_owned(), build_audio_formats());

    // Audio latencies
    dict.insert("audioLatencies".to_owned(), build_audio_latencies());

    // Display: 1920x1080 @ 60fps
    dict.insert("displays".to_owned(), build_displays());

    // Serialize to binary plist
    let root = Value::Dictionary(dict.into_iter().collect());
    let mut buf = Vec::new();
    root.to_writer_binary(&mut buf)
        .expect("plist serialization should not fail");
    buf
}

fn build_audio_formats() -> Value {
    let mut fmt = BTreeMap::new();
    fmt.insert("type".to_owned(), Value::Integer(96.into()));
    fmt.insert(
        "audioInputFormats".to_owned(),
        Value::Integer(0x01000000_i64.into()),
    );
    fmt.insert(
        "audioOutputFormats".to_owned(),
        Value::Integer(0x01000000_i64.into()),
    );
    Value::Array(vec![Value::Dictionary(fmt.into_iter().collect())])
}

fn build_audio_latencies() -> Value {
    let mut lat = BTreeMap::new();
    lat.insert("type".to_owned(), Value::Integer(96.into()));
    lat.insert(
        "audioType".to_owned(),
        Value::String("default".to_owned()),
    );
    lat.insert(
        "inputLatencyMicros".to_owned(),
        Value::Integer(0.into()),
    );
    lat.insert(
        "outputLatencyMicros".to_owned(),
        Value::Integer(0.into()),
    );
    Value::Array(vec![Value::Dictionary(lat.into_iter().collect())])
}

fn build_displays() -> Value {
    let mut disp = BTreeMap::new();
    disp.insert("width".to_owned(), Value::Integer(1920.into()));
    disp.insert("height".to_owned(), Value::Integer(1080.into()));
    disp.insert("uuid".to_owned(), Value::String("e0ff6100-0000-0000-0000-000000000000".to_owned()));
    disp.insert(
        "features".to_owned(),
        Value::Integer(0x0E.into()), // rotation + mirroring
    );
    disp.insert("widthPhysical".to_owned(), Value::Integer(0.into()));
    disp.insert("heightPhysical".to_owned(), Value::Integer(0.into()));
    disp.insert("refreshRate".to_owned(), Value::Real(60.0));
    Value::Array(vec![Value::Dictionary(disp.into_iter().collect())])
}
