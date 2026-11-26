//! An independent reader of posted arc blocks for #3442 (our own code):
//! it follows the modal motion word, the modal plane and the written
//! position over the word-address dialects (RS-274 / ISO 6983) and the
//! Heidenhain plain-language `L` / `C` blocks, and gives each arc block
//! with its start and end as written.

pub struct ArcBlock {
    pub line: String,
    pub coincident: bool,
    pub r: Option<f64>,
}

fn word(tok: &str) -> Option<(char, f64)> {
    let mut cs = tok.chars();
    let l = cs.next()?;
    if !l.is_ascii_uppercase() {
        return None;
    }
    let rest = cs.as_str();
    let rest = rest.strip_prefix('+').unwrap_or(rest);
    rest.parse::<f64>().ok().map(|v| (l, v))
}

/// The words of one token. A compact block (RichAuto `.mmg`:
/// `N60G3X18.123Y5.432I-8.000J0.000`, no spaces between its words) gives
/// each of its words; any other token is given whole.
fn split_words(tok: &str) -> Vec<&str> {
    let starts: Vec<usize> = tok.char_indices().filter(|(_, c)| c.is_ascii_uppercase()).map(|(i, _)| i).collect();
    if starts.len() < 2 || starts[0] != 0 {
        return vec![tok];
    }
    let parts: Vec<&str> =
        starts.iter().enumerate().map(|(n, &i)| &tok[i..starts.get(n + 1).copied().unwrap_or(tok.len())]).collect();
    if parts.iter().all(|p| word(p).is_some()) { parts } else { vec![tok] }
}

/// Each arc block of `text` (the program, then a `--- ` trailer). A lathe
/// program starts in G18 (its arcs are in the XZ meridian plane).
pub fn arc_blocks(text: &str, lathe: bool) -> Vec<ArcBlock> {
    let mut out = Vec::new();
    let mut pos: [Option<f64>; 3] = [None; 3];
    let (mut motion, mut plane) = (0u32, if lathe { 18u8 } else { 17u8 });
    for raw in text.lines() {
        if raw.starts_with("--- ") {
            break;
        }
        let mut code = String::new();
        let mut depth = 0;
        for ch in raw.chars() {
            match ch {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                ';' if depth == 0 => break,
                c if depth == 0 => code.push(c),
                _ => {}
            }
        }
        let toks: Vec<&str> = code.split_whitespace().flat_map(split_words).collect();
        if toks.iter().any(|t| matches!(*t, "BLK" | "FORM" | "CC" | "TOOL")) {
            continue;
        }
        for t in &toks {
            match *t {
                "L" => motion = 10,
                "C" => motion = 20,
                _ => {}
            }
        }
        let words: Vec<(char, f64)> = toks.iter().filter_map(|t| word(t)).collect();
        let gs: Vec<u32> = words.iter().filter(|(l, _)| *l == 'G').map(|(_, v)| (*v * 10.0).round() as u32).collect();
        if gs.iter().any(|g| matches!(g, 280 | 300 | 530)) {
            continue;
        }
        for g in &gs {
            match g {
                0 | 10 | 20 | 30 => motion = *g,
                170 => plane = 17,
                180 => plane = 18,
                190 => plane = 19,
                _ => {}
            }
        }
        let get = |l: char| words.iter().find(|(c, _)| *c == l).map(|(_, v)| *v);
        let xyz = [get('X'), get('Y'), get('Z')];
        if xyz.iter().all(Option::is_none) {
            continue;
        }
        let end = [xyz[0].or(pos[0]), xyz[1].or(pos[1]), xyz[2].or(pos[2])];
        if motion == 20 || motion == 30 {
            let (u, v) = match plane {
                18 => (2, 0),
                19 => (1, 2),
                _ => (0, 1),
            };
            let known = pos[u].is_some() && pos[v].is_some();
            out.push(ArcBlock {
                line: raw.trim().to_string(),
                coincident: known && end[u] == pos[u] && end[v] == pos[v],
                r: get('R'),
            });
        }
        pos = end;
    }
    out
}

/// The arc blocks whose written in-plane end is their written start.
pub fn collapsed(text: &str, lathe: bool) -> Vec<String> {
    arc_blocks(text, lathe).into_iter().filter(|a| a.coincident).map(|a| a.line).collect()
}
