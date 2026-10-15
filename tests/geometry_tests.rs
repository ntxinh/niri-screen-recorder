use niri_recorder::capture::request::Geometry;

#[test]
fn parses_slurp_output() {
    let g = Geometry::parse_slurp("100,200 800x600\n").unwrap();
    assert_eq!((g.x, g.y, g.w, g.h), (100, 200, 800, 600));
}
#[test]
fn accepts_negative_origin() {
    assert!(Geometry::parse_slurp("-1920,0 1920x1080").is_ok());
}
#[test]
fn rejects_zero_size() {
    assert!(Geometry::parse_slurp("0,0 0x600").is_err());
}
#[test]
fn rejects_garbage() {
    assert!(Geometry::parse_slurp("hello").is_err());
}
#[test]
fn wf_arg_roundtrip() {
    assert_eq!(
        Geometry::parse_slurp("1,2 3x4").unwrap().to_wf_arg(),
        "1,2 3x4"
    );
}
