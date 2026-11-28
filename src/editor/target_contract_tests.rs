use crate::replay::{ops::parse, runner::run};

#[test]
fn an_unavailable_visual_action_preserves_active_and_banked_regions() {
    let script = parse(
        "new\nkeys ialpha beta gamma<Esc>0v4l<Esc>2wv4lR\n\
         assert mode visual\nassert line 0 \"alpha beta gamma\"\n\
         keys d\nassert line 0 \" beta \"\n",
    )
    .unwrap();
    if let Err(error) = run(&script, Vec::new()) {
        panic!("{}", error.message);
    }
}
