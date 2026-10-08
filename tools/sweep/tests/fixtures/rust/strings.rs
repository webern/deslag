fn strings() {
    let a = "a // is not a comment, nor is /* this */";
    let b = r#"a "quoted" // string"#;
    let c = r##"fewer "# hashes close nothing"##;
    let d = "escaped \" quote // still inside";
    let e = b"bytes /* inside */";
    let f = br"raw bytes";
    let g = c"a C string";
    let h = "two
lines // inside the string";
}
