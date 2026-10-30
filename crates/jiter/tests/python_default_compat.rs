use jiter::Jiter;

fn codepoints(raw: &[u8]) -> Vec<u32> {
    let mut parser = Jiter::new(raw);
    let value = parser.next_python_string().unwrap();
    parser.finish().unwrap();
    value
}

#[test]
fn escaped_surrogates_preserve_codepoints_and_only_combine_adjacent_pairs() {
    for (raw, expected) in [
        (br#""\ud800""#.as_slice(), vec![0xd800]),
        (br#""\udfff""#.as_slice(), vec![0xdfff]),
        (br#""\ud800\udc00""#.as_slice(), vec![0x10000]),
        (br#""\udbff\udfff""#.as_slice(), vec![0x10ffff]),
        (br#""\ud800\u0061""#.as_slice(), vec![0xd800, 97]),
        (br#""\ud800\ud800\udc00""#.as_slice(), vec![0xd800, 0x10000]),
        (br#""\udc00\ud800""#.as_slice(), vec![0xdc00, 0xd800]),
        (br#""\ud800x\udc00""#.as_slice(), vec![0xd800, 120, 0xdc00]),
    ] {
        assert_eq!(codepoints(raw), expected);
    }
}

#[test]
fn raw_surrogatepass_is_lossless_and_invalid_utf8_is_refused() {
    assert_eq!(codepoints(b"\"\xed\xa0\x80\""), vec![0xd800]);
    assert_eq!(codepoints(b"\"a\xed\xbf\xbfz\""), vec![97, 0xdfff, 122]);
    assert_eq!(codepoints("\"é𐀀\"".as_bytes()), vec![0xe9, 0x10000]);
    for raw in [
        b"\"\xc0\x80\"".as_slice(),
        b"\"\xed\xa0\"",
        b"\"\xf4\x90\x80\x80\"",
        b"\"\x80\"",
    ] {
        assert!(Jiter::new(raw).next_python_string().is_err());
    }
}

#[test]
fn scalar_strings_reuse_json_escape_and_control_rules() {
    for raw in [
        br#""a\"b\\c\/d\b\f\n\r\t\u0000""#.as_slice(),
        br#""\u00e9\ud83d\ude00""#,
        b"\"plain text\"",
    ] {
        let expected: String = serde_json::from_slice(raw).unwrap();
        assert_eq!(codepoints(raw), expected.chars().map(u32::from).collect::<Vec<_>>());
    }
    for raw in [
        b"\"\n\"".as_slice(),
        br#""\x""#,
        br#""\u12xz""#,
        br#""\ud800\uxxxx""#,
        b"\"unterminated",
    ] {
        assert!(Jiter::new(raw).next_python_string().is_err());
    }
}

#[test]
fn object_iteration_retains_duplicate_decoded_keys_and_delimiter_checks() {
    let mut parser = Jiter::new(br#"{"a":1,"\u0061":2,"\ud800":3}"#);
    assert_eq!(parser.next_python_object().unwrap(), Some(vec![97]));
    assert_eq!(parser.next_int().unwrap(), jiter::NumberInt::Int(1));
    assert_eq!(parser.next_python_key().unwrap(), Some(vec![97]));
    assert_eq!(parser.next_int().unwrap(), jiter::NumberInt::Int(2));
    assert_eq!(parser.next_python_key().unwrap(), Some(vec![0xd800]));
    assert_eq!(parser.next_int().unwrap(), jiter::NumberInt::Int(3));
    assert_eq!(parser.next_python_key().unwrap(), None);
    parser.finish().unwrap();
    for raw in [br#"{"a" 1}"#.as_slice(), br#"{1:2}"#, b"{"] {
        assert!(Jiter::new(raw).next_python_object().is_err());
    }
    let mut trailing = Jiter::new(br#"{"a":1,}"#);
    trailing.next_python_object().unwrap();
    trailing.next_int().unwrap();
    assert!(trailing.next_python_key().is_err());
}

#[test]
fn explicit_integer_limit_is_applied_after_float_classification() {
    for raw in ["1".repeat(4300), format!("-{}", "1".repeat(4300))] {
        let mut parser = Jiter::new(raw.as_bytes());
        assert_eq!(parser.next_python_number_bytes(4300).unwrap(), raw.as_bytes());
        parser.finish().unwrap();
    }
    let integer = "1".repeat(4301);
    assert!(Jiter::new(integer.as_bytes()).next_python_number_bytes(4300).is_err());
    let mut unlimited = Jiter::new(integer.as_bytes());
    assert_eq!(unlimited.next_python_number_bytes(0).unwrap(), integer.as_bytes());
    unlimited.finish().unwrap();
    for raw in [format!("{}.5", "1".repeat(4301)), format!("{}e-5000", "1".repeat(4301))] {
        let mut parser = Jiter::new(raw.as_bytes());
        assert_eq!(parser.next_python_number_bytes(4300).unwrap(), raw.as_bytes());
        parser.finish().unwrap();
    }
}

#[test]
fn number_bytes_preserve_nonfinite_signed_zero_and_invalid_syntax() {
    for raw in ["NaN", "Infinity", "-Infinity", "-0", "-0.0", "1e10000", "0e2", "0.01"] {
        let mut parser = Jiter::new(raw.as_bytes()).with_allow_inf_nan();
        assert_eq!(parser.next_python_number_bytes(4300).unwrap(), raw.as_bytes());
        parser.finish().unwrap();
    }
    assert!(Jiter::new(b"NaN").next_python_number_bytes(4300).is_err());
    for raw in ["01", "1e", "1e+", "1.", "--1", "true", "1x", "+1"] {
        let mut parser = Jiter::new(raw.as_bytes()).with_allow_inf_nan();
        assert!(parser.next_python_number_bytes(4300).is_err() || parser.finish().is_err());
    }
}

#[test]
fn default_string_and_number_apis_keep_their_original_domain() {
    assert!(Jiter::new(br#""\ud800""#).next_str().is_err());
    assert!(Jiter::new(b"\"\xed\xa0\x80\"").next_str().is_err());
    let long = format!("{}.5", "1".repeat(4301));
    assert!(Jiter::new(long.as_bytes()).next_number_bytes().is_err());
    assert_eq!(Jiter::new(br#""hello\nworld""#).next_str().unwrap(), "hello\nworld");
}
