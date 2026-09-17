// SPDX-License-Identifier: GPL-3.0-or-later
//! Generic protobuf wire-format rewriter: walks length-delimited fields,
//! rewrites the ones that decode as UTF-8 containing the old path, and
//! fixes the varint lengths (windsurf / antigravity .pb files).

use crate::spec::ReplaceSpec;

/// nesting depth cap: protobuf data is shallow in practice; deep nesting in
/// the wild means hostile/corrupt input, not a real message (guards the
/// recursive walker against stack exhaustion)
const MAX_DEPTH: u32 = 64;

pub fn pb_replace(buf: &[u8], spec: &ReplaceSpec) -> (Vec<u8>, bool) {
    pb_replace_inner(buf, spec, 0)
}

fn pb_replace_inner(buf: &[u8], spec: &ReplaceSpec, depth: u32) -> (Vec<u8>, bool) {
    if depth > MAX_DEPTH {
        return (buf.to_vec(), false);
    }
    let mut out = Vec::with_capacity(buf.len());
    let mut i = 0usize;
    let mut changed = false;
    let n = buf.len();
    while i < n {
        let (key, ni) = match read_varint(buf, i) {
            Some(v) => v,
            None => return (buf.to_vec(), false),
        };
        i = ni;
        let wire = key & 7;
        match wire {
            0 => {
                let (val, ni) = match read_varint(buf, i) {
                    Some(v) => v,
                    None => return (buf.to_vec(), false),
                };
                i = ni;
                encode_varint(&mut out, key);
                encode_varint(&mut out, val);
            }
            2 => {
                let (ln, ni) = match read_varint(buf, i) {
                    Some(v) => v,
                    None => return (buf.to_vec(), false),
                };
                i = ni;
                // checked length: a corrupt u64::MAX varint must not
                // overflow the addition, just abort unchanged
                let ln = match usize::try_from(ln) {
                    Ok(ln) => ln,
                    Err(_) => return (buf.to_vec(), false),
                };
                if ln > n - i {
                    return (buf.to_vec(), false);
                }
                let payload = &buf[i..i + ln];
                i += ln;
                let (new_payload, sub_changed) = handle_payload(payload, spec, depth);
                changed |= sub_changed;
                encode_varint(&mut out, key);
                encode_varint(&mut out, new_payload.len() as u64);
                out.extend_from_slice(&new_payload);
            }
            1 => {
                if i + 8 > n {
                    return (buf.to_vec(), false);
                }
                encode_varint(&mut out, key);
                out.extend_from_slice(&buf[i..i + 8]);
                i += 8;
            }
            5 => {
                if i + 4 > n {
                    return (buf.to_vec(), false);
                }
                encode_varint(&mut out, key);
                out.extend_from_slice(&buf[i..i + 4]);
                i += 4;
            }
            _ => return (buf.to_vec(), false),
        }
    }
    (out, changed)
}

fn handle_payload(payload: &[u8], spec: &ReplaceSpec, depth: u32) -> (Vec<u8>, bool) {
    if !spec.maybe_contains(payload) {
        // may still be a nested message containing strings deeper down
        let (sub, ch) = pb_replace_inner(payload, spec, depth + 1);
        if ch {
            return (sub, true);
        }
        return (payload.to_vec(), false);
    }
    if let Ok(text) = std::str::from_utf8(payload) {
        let new_text = spec.replace(text);
        if new_text != text {
            return (new_text.into_bytes(), true);
        }
        return (payload.to_vec(), false);
    }
    // nested message containing the path deeper inside
    let (sub, ch) = pb_replace_inner(payload, spec, depth + 1);
    if ch {
        return (sub, true);
    }
    (payload.to_vec(), false)
}

fn read_varint(buf: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0u32;
    loop {
        let b = *buf.get(i)?;
        i += 1;
        result |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some((result, i));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

fn encode_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            out.push(b | 0x80);
        } else {
            out.push(b);
            return;
        }
    }
}
