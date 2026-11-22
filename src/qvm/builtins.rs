//! QORVYNT standard built-in function dispatch.
//!
//! This module owns the built-in implementations used by the VM. Host-side
//! parsing and compiler code can call the same dispatch without duplicating it.

use crate::qvm::Value;
use crate::qvm::runtime_support::{check_map_capacity, value_to_string};
use crate::qvm::limits::{MAX_BYTECODE_STRING_BYTES, MAX_COLLECTION_ITEMS};

fn hex_encode(input: &str) -> String {
    if input.is_empty() { return "-".into(); }
    input.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn builtin_unchecked(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let arg_len = args.len();
    let arity = |expected: usize| if arg_len == expected { Ok(()) } else { Err(format!("builtin '{name}' expects {expected} argument(s), got {arg_len}")) };
    match name {
        "array" => Ok(Value::Array(args)),
        "map" => {
            if !args.len().is_multiple_of(2) { return Err("builtin 'map' expects alternating string keys and values".into()); }
            let mut result = std::collections::BTreeMap::new();
            let mut it = args.into_iter();
            while let Some(key) = it.next() {
                let Value::Str(key) = key else { return Err("builtin 'map' keys must be strings".into()); };
                let value = it.next().ok_or("missing map value")?;
                result.insert(key, value);
            }
            Ok(Value::Map(result))
        }
        "len" => {
            arity(1)?;
            let n = match &args[0] { Value::Array(v) => v.len(), Value::Map(v) => v.len(), Value::Str(v) => v.chars().count(), _ => return Err("type error: len expects an array, map, or string".into()) };
            Ok(Value::Int(i64::try_from(n).map_err(|_| "length exceeds integer range")?))
        }
        "get" => {
            arity(2)?;
            let mut values = args.into_iter();
            let collection = values.next().ok_or("get expects a collection and an index")?;
            let index = values.next().ok_or("get expects a collection and an index")?;
            match (collection, index) {
                (Value::Array(items), Value::Int(index)) if index >= 0 => items.into_iter().nth(index as usize).ok_or_else(|| format!("array index {index} out of bounds")),
                (Value::Array(_), Value::Int(index)) => Err(format!("array index {index} out of bounds")),
                (Value::Map(mut items), Value::Str(key)) => items.remove(&key).ok_or_else(|| format!("map key '{key}' not found")),
                (Value::Array(_), _) => Err("type error: array index must be an integer".into()),
                (Value::Map(_), _) => Err("type error: map key must be a string".into()),
                _ => Err("type error: get expects an array or map as its first argument".into()),
            }
        }
        "contains" => {
            arity(2)?;
            let found = match (&args[0], &args[1]) {
                (Value::Array(items), needle) => items.contains(needle),
                (Value::Map(items), Value::Str(key)) => items.contains_key(key),
                (Value::Map(_), _) => return Err("type error: map contains expects a string key".into()),
                _ => return Err("type error: contains expects an array or map".into()),
            };
            Ok(Value::Bool(found))
        }
        "push" => {
            arity(2)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or("push expects an array and a value")?;
            let value = values.next().ok_or("push expects an array and a value")?;
            let Value::Array(mut items) = original else { return Err("type error: push expects an array as its first argument".into()); };
            if items.len() >= MAX_COLLECTION_ITEMS {
                return Err(format!("array exceeds safety limit ({MAX_COLLECTION_ITEMS} items)"));
            }
            items.push(value);
            Ok(Value::Array(items))
        }
        "put" => {
            arity(3)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or("put expects a map, key, and value")?;
            let key = values.next().ok_or("put expects a map, key, and value")?;
            let value = values.next().ok_or("put expects a map, key, and value")?;
            let Value::Map(mut items) = original else { return Err("type error: put expects a map as its first argument".into()); };
            let Value::Str(key) = key else { return Err("type error: put key must be a string".into()); };
            check_map_capacity(&items, &key)?;
            items.insert(key, value);
            Ok(Value::Map(items))
        }
        "keys" | "values" => {
            arity(1)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or("keys/values expects a map")?;
            let Value::Map(items) = original else { return Err(format!("type error: {name} expects a map")); };
            let values = if name == "keys" { items.into_keys().map(Value::Str).collect() } else { items.into_values().collect() };
            Ok(Value::Array(values))
        }
        "type_of" => {
            arity(1)?;
            let kind = match &args[0] {
                Value::Int(_) => "integer",
                Value::Str(_) => "string",
                Value::Bool(_) => "boolean",
                Value::Array(_) => "array",
                Value::Map(_) => "map",
            };
            Ok(Value::Str(kind.into()))
        }
        "to_string" => {
            arity(1)?;
            Ok(Value::Str(value_to_string(&args[0])))
        }
        "abs" => {
            arity(1)?;
            match args[0] {
                Value::Int(value) => Ok(Value::Int(value.checked_abs().ok_or("integer overflow in abs")?)),
                _ => Err("type error: abs expects an integer".into()),
            }
        }
        "min" | "max" => {
            arity(2)?;
            match (&args[0], &args[1]) {
                (Value::Int(left), Value::Int(right)) => Ok(Value::Int(if name == "min" { (*left).min(*right) } else { (*left).max(*right) })),
                _ => Err(format!("type error: {name} expects two integers")),
            }
        }
        "dna_complement" => {
            arity(1)?;
            let Value::Str(sequence) = &args[0] else {
                return Err("type error: dna_complement expects a DNA string".into());
            };
            let mut result = String::with_capacity(sequence.len());
            for base in sequence.chars() {
                result.push(match base.to_ascii_uppercase() {
                    'A' => 'T', 'T' => 'A', 'C' => 'G', 'G' => 'C',
                    _ => return Err(format!("dna_complement found invalid DNA base '{base}'")),
                });
            }
            Ok(Value::Str(result))
        }
        "gc_content_percent" => {
            arity(1)?;
            let Value::Str(sequence) = &args[0] else {
                return Err("type error: gc_content_percent expects a DNA string".into());
            };
            if sequence.is_empty() { return Err("gc_content_percent cannot use an empty DNA string".into()); }
            let mut gc = 0i64;
            for base in sequence.chars() {
                match base.to_ascii_uppercase() {
                    'G' | 'C' => gc += 1,
                    'A' | 'T' => {},
                    _ => return Err(format!("gc_content_percent found invalid DNA base '{base}'")),
                }
            }
            let length = i64::try_from(sequence.chars().count()).map_err(|_| "DNA sequence is too long")?;
            Ok(Value::Int(gc.checked_mul(100).ok_or("GC percentage overflow")? / length))
        }
        "molar_mass" => {
            arity(1)?;
            let Value::Array(atomic_masses) = &args[0] else {
                return Err("type error: molar_mass expects an array of integer atomic masses".into());
            };
            if atomic_masses.is_empty() { return Err("molar_mass expects at least one atomic mass".into()); }
            let mut total = 0i64;
            for mass in atomic_masses {
                let Value::Int(mass) = mass else {
                    return Err("type error: molar_mass expects an array containing only integers".into());
                };
                if *mass < 0 { return Err("molar_mass atomic masses cannot be negative".into()); }
                total = total.checked_add(*mass).ok_or("integer overflow in molar_mass")?;
            }
            Ok(Value::Int(total))
        }
        "gcd" => {
            arity(2)?;
            let (Value::Int(mut a), Value::Int(mut b)) = (&args[0], &args[1]) else {
                return Err("type error: gcd expects two integers".into());
            };
            a = a.checked_abs().ok_or("integer overflow in gcd")?;
            b = b.checked_abs().ok_or("integer overflow in gcd")?;
            while b != 0 { let remainder = a % b; a = b; b = remainder; }
            Ok(Value::Int(a))
        }
        "lcm" => {
            arity(2)?;
            let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
                return Err("type error: lcm expects two integers".into());
            };
            if *a == 0 || *b == 0 { return Ok(Value::Int(0)); }
            let mut x = a.checked_abs().ok_or("integer overflow in lcm")?;
            let mut y = b.checked_abs().ok_or("integer overflow in lcm")?;
            let mut p = x;
            while y != 0 { let r = p % y; p = y; y = r; }
            x = a.checked_div(p).ok_or("integer overflow in lcm")?;
            Ok(Value::Int(x.checked_mul(*b).and_then(i64::checked_abs).ok_or("integer overflow in lcm")?))
        }
        "factorial" => {
            arity(1)?;
            let Value::Int(n) = args[0] else { return Err("type error: factorial expects an integer".into()); };
            if !(0..=20).contains(&n) { return Err("factorial expects an integer from 0 through 20".into()); }
            let mut result = 1i64;
            for value in 2..=n { result = result.checked_mul(value).ok_or("integer overflow in factorial")?; }
            Ok(Value::Int(result))
        }
        "is_prime" => {
            arity(1)?;
            let Value::Int(n) = args[0] else { return Err("type error: is_prime expects an integer".into()); };
            if n < 2 { return Ok(Value::Bool(false)); }
            if n == 2 { return Ok(Value::Bool(true)); }
            if n % 2 == 0 { return Ok(Value::Bool(false)); }
            let mut divisor = 3i64;
            while divisor <= n / divisor {
                if n % divisor == 0 { return Ok(Value::Bool(false)); }
                divisor += 2;
            }
            Ok(Value::Bool(true))
        }
        "fibonacci" => {
            arity(1)?;
            let Value::Int(n) = args[0] else { return Err("type error: fibonacci expects an integer".into()); };
            if !(0..=92).contains(&n) { return Err("fibonacci expects an integer from 0 through 92".into()); }
            let (mut a, mut b) = (0i64, 1i64);
            for _ in 0..n { let next = a.checked_add(b).ok_or("integer overflow in fibonacci")?; a = b; b = next; }
            Ok(Value::Int(a))
        }
        "pow_int" => {
            arity(2)?;
            let (Value::Int(base), Value::Int(exponent)) = (&args[0], &args[1]) else {
                return Err("type error: pow_int expects two integers".into());
            };
            if !(0..=63).contains(exponent) { return Err("pow_int exponent must be from 0 through 63".into()); }
            let mut result = 1i64;
            let mut factor = *base;
            let mut power = *exponent as u32;
            while power > 0 {
                if power & 1 == 1 { result = result.checked_mul(factor).ok_or("integer overflow in pow_int")?; }
                power >>= 1;
                if power > 0 { factor = factor.checked_mul(factor).ok_or("integer overflow in pow_int")?; }
            }
            Ok(Value::Int(result))
        }
        "mean" => {
            arity(1)?;
            let Value::Array(items) = &args[0] else { return Err("type error: mean expects an array of integers".into()); };
            if items.is_empty() { return Err("mean cannot be used with an empty array".into()); }
            let mut total = 0i64;
            for item in items {
                let Value::Int(value) = item else { return Err("type error: mean expects an array containing only integers".into()); };
                total = total.checked_add(*value).ok_or("integer overflow in mean")?;
            }
            Ok(Value::Int(total / i64::try_from(items.len()).map_err(|_| "mean item count overflow")?))
        }
        "speed" | "density" | "molarity" => {
            arity(2)?;
            let (Value::Int(numerator), Value::Int(denominator)) = (&args[0], &args[1]) else {
                return Err(format!("type error: {name} expects two integers"));
            };
            if *denominator <= 0 { return Err(format!("{name} denominator must be positive")); }
            Ok(Value::Int(numerator.checked_div(*denominator).ok_or("integer overflow in scientific calculation")?))
        }
        "force" => {
            arity(2)?;
            let (Value::Int(mass), Value::Int(acceleration)) = (&args[0], &args[1]) else {
                return Err("type error: force expects mass and acceleration as integers".into());
            };
            Ok(Value::Int(mass.checked_mul(*acceleration).ok_or("integer overflow in force")?))
        }
        "kinetic_energy" => {
            arity(2)?;
            let (Value::Int(mass), Value::Int(velocity)) = (&args[0], &args[1]) else {
                return Err("type error: kinetic_energy expects mass and velocity as integers".into());
            };
            if *mass < 0 { return Err("kinetic_energy mass cannot be negative".into()); }
            let velocity_squared = velocity.checked_mul(*velocity).ok_or("integer overflow in kinetic_energy")?;
            Ok(Value::Int(mass.checked_mul(velocity_squared).ok_or("integer overflow in kinetic_energy")? / 2))
        }
        "bmi_x10" => {
            arity(2)?;
            let (Value::Int(mass_kg), Value::Int(height_cm)) = (&args[0], &args[1]) else {
                return Err("type error: bmi_x10 expects mass in kg and height in cm as integers".into());
            };
            if *mass_kg <= 0 || *height_cm <= 0 { return Err("bmi_x10 mass and height must be positive".into()); }
            let numerator = mass_kg.checked_mul(100_000).ok_or("integer overflow in bmi_x10")?;
            let denominator = height_cm.checked_mul(*height_cm).ok_or("integer overflow in bmi_x10")?;
            Ok(Value::Int(numerator / denominator))
        }
        "range" => {
            arity(2)?;
            let (Value::Int(start), Value::Int(end)) = (&args[0], &args[1]) else {
                return Err("type error: range expects two integers".into());
            };
            let length = end.checked_sub(*start).ok_or("range bounds overflow")?;
            if length < 0 { return Err("range end must be greater than or equal to start".into()); }
            const MAX_RANGE_ITEMS: i64 = 100_000;
            if length > MAX_RANGE_ITEMS { return Err(format!("range exceeds safety limit ({MAX_RANGE_ITEMS} items)")); }
            let mut items = Vec::with_capacity(length as usize);
            let mut current = *start;
            while current < *end {
                items.push(Value::Int(current));
                current = current.checked_add(1).ok_or("integer overflow while building range")?;
            }
            Ok(Value::Array(items))
        }
        "hex_encode" => {
            arity(1)?;
            let Value::Str(value) = &args[0] else {
                return Err("type error: hex_encode expects a string".into());
            };
            if value.len() > MAX_BYTECODE_STRING_BYTES {
                return Err("hex_encode input exceeds string safety limit".into());
            }
            Ok(Value::Str(hex_encode(value)))
        }
        "char_at" => {
            arity(2)?;
            let (Value::Str(value), Value::Int(index)) = (&args[0], &args[1]) else {
                return Err("type error: char_at expects a string and a non-negative integer index".into());
            };
            if *index < 0 {
                return Err("char_at index out of bounds".into());
            }
            value.chars().nth(*index as usize)
                .map(|character| Value::Str(character.to_string()))
                .ok_or_else(|| "char_at index out of bounds".into())
        }
        "index_of" => {
            arity(2)?;
            match (&args[0], &args[1]) {
                (Value::Str(text), Value::Str(needle)) => {
                    let index = text.find(needle).map(|byte_index| text[..byte_index].chars().count());
                    Ok(Value::Int(index.and_then(|n| i64::try_from(n).ok()).unwrap_or(-1)))
                }
                (Value::Array(items), needle) => {
                    let index = items.iter().position(|item| item == needle);
                    Ok(Value::Int(index.and_then(|n| i64::try_from(n).ok()).unwrap_or(-1)))
                }
                (Value::Str(_), _) => Err("type error: index_of expects a string needle when searching a string".into()),
                _ => Err("type error: index_of expects a string or array as its first argument".into()),
            }
        }
        "trim" | "lower" | "upper" | "char_count" => {
            arity(1)?;
            let Value::Str(value) = &args[0] else {
                return Err(format!("type error: {name} expects a string"));
            };
            match name {
                "trim" => Ok(Value::Str(value.trim().to_string())),
                "lower" => {
                    let result = value.to_lowercase();
                    if result.len() > MAX_BYTECODE_STRING_BYTES { return Err("lower result exceeds string safety limit".into()); }
                    Ok(Value::Str(result))
                }
                "upper" => {
                    let result = value.to_uppercase();
                    if result.len() > MAX_BYTECODE_STRING_BYTES { return Err("upper result exceeds string safety limit".into()); }
                    Ok(Value::Str(result))
                }
                "char_count" => Ok(Value::Int(i64::try_from(value.chars().count()).map_err(|_| "character count overflow")?)),
                _ => unreachable!(),
            }
        }
        "to_int" => {
            arity(1)?;
            match &args[0] {
                Value::Int(value) => Ok(Value::Int(*value)),
                Value::Str(value) => value.trim().parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| format!("cannot convert '{value}' to a 64-bit integer")),
                _ => Err("type error: to_int expects an integer or numeric string".into()),
            }
        }
        "first" | "last" => {
            arity(1)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or_else(|| format!("{name} expects an array"))?;
            let Value::Array(items) = original else { return Err(format!("type error: {name} expects an array")); };
            if items.is_empty() { return Err(format!("{name} cannot be used with an empty array")); }
            if name == "first" {
                items.into_iter().next().ok_or_else(|| format!("{name} cannot be used with an empty array"))
            } else {
                items.into_iter().last().ok_or_else(|| format!("{name} cannot be used with an empty array"))
            }
        }
        "reverse" => {
            arity(1)?;
            let mut values = args.into_iter();
            let original = values.next().ok_or("reverse expects an array or string")?;
            match original {
                Value::Array(mut items) => { items.reverse(); Ok(Value::Array(items)) }
                Value::Str(value) => Ok(Value::Str(value.chars().rev().collect())),
                _ => Err("type error: reverse expects an array or string".into()),
            }
        }
        "is_empty" => {
            arity(1)?;
            let empty = match &args[0] {
                Value::Array(items) => items.is_empty(),
                Value::Map(items) => items.is_empty(),
                Value::Str(value) => value.is_empty(),
                _ => return Err("type error: is_empty expects an array, map, or string".into()),
            };
            Ok(Value::Bool(empty))
        }
        "clamp" => {
            arity(3)?;
            let (Value::Int(value), Value::Int(minimum), Value::Int(maximum)) = (&args[0], &args[1], &args[2]) else {
                return Err("type error: clamp expects three integers".into());
            };
            if minimum > maximum { return Err("clamp minimum must be less than or equal to maximum".into()); }
            Ok(Value::Int((*value).clamp(*minimum, *maximum)))
        }
        "sum" => {
            arity(1)?;
            let Value::Array(items) = &args[0] else { return Err("type error: sum expects an array of integers".into()); };
            let mut total = 0i64;
            for item in items {
                let Value::Int(value) = item else { return Err("type error: sum expects an array containing only integers".into()); };
                total = total.checked_add(*value).ok_or("integer overflow in sum")?;
            }
            Ok(Value::Int(total))
        }
        "starts_with" | "ends_with" => {
            arity(2)?;
            let (Value::Str(value), Value::Str(pattern)) = (&args[0], &args[1]) else {
                return Err(format!("type error: {name} expects two strings"));
            };
            Ok(Value::Bool(if name == "starts_with" { value.starts_with(pattern) } else { value.ends_with(pattern) }))
        }
        "replace" => {
            arity(3)?;
            let (Value::Str(value), Value::Str(from), Value::Str(to)) = (&args[0], &args[1], &args[2]) else {
                return Err("type error: replace expects three strings".into());
            };
            if from.is_empty() { return Err("replace search string cannot be empty".into()); }
            let matches = value.matches(from).count();
            let removed = matches.checked_mul(from.len()).ok_or("replace size calculation overflow")?;
            let added = matches.checked_mul(to.len()).ok_or("replace size calculation overflow")?;
            let result_len = value.len().checked_sub(removed).and_then(|len| len.checked_add(added))
                .ok_or("replace result size calculation overflow")?;
            if result_len > MAX_BYTECODE_STRING_BYTES {
                return Err(format!("replace result exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
            }
            Ok(Value::Str(value.replace(from, to)))
        }
        "split" => {
            arity(2)?;
            let (Value::Str(value), Value::Str(separator)) = (&args[0], &args[1]) else {
                return Err("type error: split expects two strings".into());
            };
            if separator.is_empty() { return Err("split separator cannot be empty".into()); }
            const MAX_SPLIT_ITEMS: usize = 100_000;
            let mut parts = Vec::new();
            for part in value.split(separator) {
                if parts.len() >= MAX_SPLIT_ITEMS {
                    return Err(format!("split exceeds safety limit ({MAX_SPLIT_ITEMS} items)"));
                }
                parts.push(Value::Str(part.to_string()));
            }
            Ok(Value::Array(parts))
        }
        "join" => {
            arity(2)?;
            let (Value::Array(items), Value::Str(separator)) = (&args[0], &args[1]) else {
                return Err("type error: join expects an array of strings and a string separator".into());
            };
            let mut parts = Vec::with_capacity(items.len());
            let mut result_len = 0usize;
            for item in items {
                let Value::Str(value) = item else { return Err("type error: join expects an array containing only strings".into()); };
                result_len = result_len.checked_add(value.len()).ok_or("join result size calculation overflow")?;
                parts.push(value.as_str());
            }
            if parts.len() > 1 {
                result_len = result_len.checked_add(separator.len().checked_mul(parts.len() - 1)
                    .ok_or("join result size calculation overflow")?).ok_or("join result size calculation overflow")?;
            }
            if result_len > MAX_BYTECODE_STRING_BYTES {
                return Err(format!("join result exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
            }
            Ok(Value::Str(parts.join(separator)))
        }
        "assert_eq" => {
            if !(2..=3).contains(&arg_len) {
                return Err(format!("builtin 'assert_eq' expects 2 or 3 argument(s), got {arg_len}"));
            }
            if args[0] == args[1] {
                Ok(Value::Bool(true))
            } else {
                let message = if args.len() == 3 {
                    value_to_string(&args[2])
                } else {
                    format!("assert_eq failed: left was {}, right was {}", value_to_string(&args[0]), value_to_string(&args[1]))
                };
                Err(message)
            }
        }
        "unique" => {
            arity(1)?;
            let Value::Array(items) = &args[0] else {
                return Err("type error: unique expects an array".into());
            };
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                if !result.contains(item) {
                    result.push(item.clone());
                }
            }
            Ok(Value::Array(result))
        }
        "assert" => {
            if !(1..=2).contains(&arg_len) {
                return Err(format!("builtin 'assert' expects 1 or 2 argument(s), got {arg_len}"));
            }
            match &args[0] {
                Value::Bool(true) => Ok(Value::Bool(true)),
                Value::Bool(false) => {
                    let message = if args.len() == 2 { value_to_string(&args[1]) } else { "assertion failed".into() };
                    Err(message)
                }
                _ => Err("type error: assert expects a boolean condition".into()),
            }
        }
        _ => Err(format!("unknown builtin '{name}'")),
    }
}
