use crate::replay::ops::parse;
use crate::replay::runner::run;

fn check(script: &str) {
    let ops = parse(script).unwrap();
    if let Err(err) = run(&ops, Vec::new()) {
        panic!("{}", err.message);
    }
}

#[test]
fn dollar_rests_on_last_char() {
    check("new\nkeys iabc<Esc>0$\nassert cursor 0 2\n");
}

#[test]
fn dollar_x_deletes_last_char() {
    check("new\nkeys iabc<Esc>0$x\nassert line 0 \"ab\"\nassert cursor 0 1\n");
}

#[test]
fn dollar_xx_does_not_join_lines() {
    check(
        "new\nkeys iab<CR>cd<Esc>k$xx\n\
         assert line 0 \"\"\nassert line 1 \"cd\"\nassert cursor 0 0\n",
    );
}

#[test]
fn dd_on_last_line_lands_on_line_start() {
    check("new\nkeys ia<CR>b<CR>c<Esc>Gdd\nassert cursor 1 0\n");
}

#[test]
fn l_stops_on_last_char() {
    check("new\nkeys iabc<Esc>0llll\nassert cursor 0 2\n");
}

#[test]
fn undo_of_append_rests_on_last_char() {
    check("new\nkeys iabc<Esc>Axyz<Esc>u\nassert line 0 \"abc\"\nassert cursor 0 2\n");
}

#[test]
fn leaving_visual_after_dollar_rests_on_last_char() {
    check("new\nkeys iabc<Esc>0v$<Esc>\nassert mode normal\nassert cursor 0 2\n");
}
