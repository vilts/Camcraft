//! Minimal pure-Rust XML tree reader and text escaper for Hurco `.TCM`
//! files (#2566); implemented directly from the W3C XML 1.0 grammar with
//! no external dependencies.

// Controller tables have shallow trees. Bound recursion and recursive tree drops.
const MAX_XML_DEPTH: usize = 32;

/// One XML element with its tag name, text content, and children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct XmlNode {
    pub tag: String,
    pub text: String,
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    pub(crate) fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.tag == name)
    }
}

pub(crate) fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn decode_entities(raw: &str) -> Result<String, String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(semi) = after.find(';') else {
            let near: String = rest[amp..].chars().take(20).collect();
            return Err(format!("unterminated XML entity near '{near}'"));
        };
        let entity = &after[..semi];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                let code = u32::from_str_radix(&entity[2..], 16)
                    .map_err(|_| format!("invalid XML numeric entity '&{entity};'"))?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| format!("invalid XML character code '&{entity};'"))?;
                out.push(ch);
            }
            _ if entity.starts_with('#') => {
                let code: u32 = entity[1..]
                    .parse()
                    .map_err(|_| format!("invalid XML numeric entity '&{entity};'"))?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| format!("invalid XML character code '&{entity};'"))?;
                out.push(ch);
            }
            other => return Err(format!("unsupported XML entity '&{other};'")),
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Parse one root XML element, skipping BOM, declarations, comments, and whitespace.
pub(crate) fn parse_xml(input: &str) -> Result<XmlNode, String> {
    let mut pos = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    loop {
        pos = pos.trim_start();
        if pos.starts_with("<?") {
            let Some(end) = pos.find("?>") else {
                return Err("unterminated XML processing instruction".into());
            };
            pos = &pos[end + 2..];
        } else if pos.starts_with("<!--") {
            let Some(end) = pos.find("-->") else {
                return Err("unterminated XML comment".into());
            };
            pos = &pos[end + 3..];
        } else {
            break;
        }
    }
    if !pos.starts_with('<') {
        return Err("not an XML file (missing root '<' element)".into());
    }
    let (node, rest) = parse_element(pos, 1)?;
    if !rest.trim().is_empty() && !rest.trim_start().starts_with("<!--") {
        return Err("unexpected trailing content after root XML element".into());
    }
    Ok(node)
}

fn parse_element(input: &str, depth: usize) -> Result<(XmlNode, &str), String> {
    if depth > MAX_XML_DEPTH {
        return Err(format!("XML nesting exceeds the limit of {MAX_XML_DEPTH} elements"));
    }
    let after_lt = input.strip_prefix('<').ok_or("expected '<'")?;
    let (tag, mut rest) = take_name(after_lt)?;
    loop {
        rest = rest.trim_start();
        if let Some(after_self) = rest.strip_prefix("/>") {
            return Ok((
                XmlNode { tag: tag.to_owned(), text: String::new(), children: Vec::new() },
                after_self,
            ));
        }
        if let Some(after_gt) = rest.strip_prefix('>') {
            rest = after_gt;
            break;
        }
        let (key, r1) = take_name(rest)?;
        let r1 = r1.trim_start();
        let r1 = r1
            .strip_prefix('=')
            .ok_or_else(|| format!("expected '=' after attribute '{key}' on <{tag}>"))?
            .trim_start();
        let quote = r1
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
            .ok_or_else(|| format!("expected quoted value for attribute '{key}' on <{tag}>"))?;
        let after_q = &r1[1..];
        let Some(end_q) = after_q.find(quote) else {
            return Err(format!("unclosed attribute '{key}' on <{tag}>"));
        };
        let _ = decode_entities(&after_q[..end_q])?;
        rest = &after_q[end_q + 1..];
    }

    let mut text = String::new();
    let mut children = Vec::new();
    loop {
        let Some(lt) = rest.find('<') else {
            return Err(format!("unclosed XML element <{tag}>"));
        };
        if lt > 0 {
            text.push_str(&decode_entities(&rest[..lt])?);
        }
        rest = &rest[lt..];
        if let Some(after_close) = rest.strip_prefix("</") {
            let (close_tag, after_name) = take_name(after_close)?;
            let after_ws = after_name.trim_start();
            let after_gt = after_ws
                .strip_prefix('>')
                .ok_or_else(|| format!("expected '>' in closing tag </{close_tag}>"))?;
            if close_tag != tag {
                return Err(format!("mismatched XML closing tag </{close_tag}> for <{tag}>"));
            }
            return Ok((XmlNode { tag: tag.to_owned(), text, children }, after_gt));
        }
        if rest.starts_with("<!--") {
            let Some(end) = rest.find("-->") else {
                return Err("unterminated XML comment".into());
            };
            rest = &rest[end + 3..];
            continue;
        }
        if let Some(after_cdata) = rest.strip_prefix("<![CDATA[") {
            let Some(end) = after_cdata.find("]]>") else {
                return Err("unterminated XML CDATA section".into());
            };
            text.push_str(&after_cdata[..end]);
            rest = &after_cdata[end + 3..];
            continue;
        }
        let (child, after_child) = parse_element(rest, depth + 1)?;
        children.push(child);
        rest = after_child;
    }
}

fn take_name(s: &str) -> Result<(&str, &str), String> {
    let end = s
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || matches!(c, '>' | '/' | '=' | '<'))
        .map_or(s.len(), |(i, _)| i);
    if end == 0 {
        return Err("missing XML tag or attribute name".into());
    }
    Ok((&s[..end], &s[end..]))
}
