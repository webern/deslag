fn chars<'a>(x: &'a str, y: &'static str) -> char {
    let a = 'x';
    let b = '\'';
    let c = '"'; // a quote that is a char
    let d = b'z';
    let e = b'\\';
    let f = '/';
    'outer: loop {
        break 'outer;
    }
    'a'
}
