//! QORVYNT Universal Runtime (QUR) Web3 manifest v1.
//! Validates declarative resources; it does not deploy chains or create assets.
use std::collections::{BTreeMap, BTreeSet};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_DECLARATIONS: usize = 256;
const MAX_PROPERTIES_PER_DECLARATION: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainSpec { pub name: String, pub consensus: String, pub native_token: String }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpec { pub name: String, pub standard: String, pub supply: u64 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaoSpec { pub name: String, pub governance: String }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletSpec { pub name: String, pub network: String }
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QurManifest {
    pub chains: Vec<ChainSpec>, pub tokens: Vec<TokenSpec>, pub daos: Vec<DaoSpec>, pub wallets: Vec<WalletSpec>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind { Chain, Token, Dao, Wallet }
impl Kind {
    fn parse(value: &str) -> Option<Self> {
        match value { "chain" => Some(Self::Chain), "token" => Some(Self::Token), "dao" => Some(Self::Dao), "wallet" => Some(Self::Wallet), _ => None }
    }
    fn label(self) -> &'static str {
        match self { Self::Chain => "chain", Self::Token => "token", Self::Dao => "dao", Self::Wallet => "wallet" }
    }
}
#[derive(Debug)]
struct Declaration { kind: Kind, name: String, line: usize, fields: BTreeMap<String, (String, bool)> }
fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if quoted {
            if escaped { escaped = false; }
            else if ch == '\\' { escaped = true; }
            else if ch == '"' { quoted = false; }
        } else if ch == '"' { quoted = true; }
        else if ch == '#' { return &line[..index]; }
    }
    line
}
fn parse_value(raw: &str, line: usize) -> Result<(String, bool), String> {
    let value = raw.trim();
    if value.is_empty() { return Err(format!("line {line}: missing property value")); }
    if value.starts_with('"') {
        if !value.ends_with('"') || value.len() < 2 { return Err(format!("line {line}: malformed quoted value")); }
        let inner = &value[1..value.len()-1];
        if inner.contains('"') || inner.contains('\\') || inner.contains('\n') {
            return Err(format!("line {line}: escaped strings are not supported in QUR manifest v1"));
        }
        if inner.is_empty() { return Err(format!("line {line}: empty string values are not allowed")); }
        Ok((inner.to_string(), true))
    } else {
        if value.chars().any(char::is_whitespace) || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!("line {line}: expected an identifier, integer, or quoted string"));
        }
        Ok((value.to_string(), false))
    }
}
fn required<'a>(decl: &'a Declaration, key: &str) -> Result<&'a (String, bool), String> {
    decl.fields.get(key).ok_or_else(|| format!("line {}: {} '{}' is missing required property '{key}'", decl.line, decl.kind.label(), decl.name))
}
fn string_field(decl: &Declaration, key: &str) -> Result<String, String> {
    let (value, quoted) = required(decl, key)?;
    if !quoted { return Err(format!("line {}: property '{key}' must be a quoted string", decl.line)); }
    Ok(value.clone())
}
fn validate_fields(decl: &Declaration) -> Result<(), String> {
    let allowed: &[&str] = match decl.kind {
        Kind::Chain => &["consensus", "native_token"], Kind::Token => &["standard", "supply"],
        Kind::Dao => &["governance"], Kind::Wallet => &["network"],
    };
    for key in decl.fields.keys() {
        if !allowed.contains(&key.as_str()) { return Err(format!("line {}: unknown property '{key}' for {} '{}'", decl.line, decl.kind.label(), decl.name)); }
    }
    Ok(())
}
pub fn parse(source: &str) -> Result<QurManifest, String> {
    if source.len() > MAX_SOURCE_BYTES { return Err(format!("QUR manifest exceeds {MAX_SOURCE_BYTES} byte safety limit")); }
    let mut declarations = Vec::new();
    let mut current: Option<Declaration> = None;
    for (index, raw) in source.lines().enumerate() {
        let line_no = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() { continue; }
        if line == "}" {
            declarations.push(current.take().ok_or_else(|| format!("line {line_no}: closing brace without a declaration"))?);
            continue;
        }
        if current.is_none() {
            let header = line.strip_suffix('{').ok_or_else(|| format!("line {line_no}: expected 'chain|token|dao|wallet Name {{'"))?.trim();
            let mut parts = header.split_whitespace();
            let kind_text = parts.next().unwrap_or("");
            let name = parts.next().unwrap_or("");
            if parts.next().is_some() { return Err(format!("line {line_no}: malformed declaration header")); }
            let kind = Kind::parse(kind_text).ok_or_else(|| format!("line {line_no}: unsupported QUR declaration '{kind_text}'"))?;
            if !valid_identifier(name) { return Err(format!("line {line_no}: invalid {} name '{name}'", kind.label())); }
            if declarations.len() + 1 > MAX_DECLARATIONS { return Err("QUR manifest declaration limit exceeded".into()); }
            current = Some(Declaration { kind, name: name.to_string(), line: line_no, fields: BTreeMap::new() });
            continue;
        }
        let decl = current.as_mut().ok_or("internal QUR parser state error")?;
        if decl.fields.len() >= MAX_PROPERTIES_PER_DECLARATION { return Err(format!("line {line_no}: too many properties in {} '{}'", decl.kind.label(), decl.name)); }
        let (key, raw_value) = line.split_once('=').ok_or_else(|| format!("line {line_no}: expected property = value"))?;
        let key = key.trim();
        if !valid_identifier(key) { return Err(format!("line {line_no}: invalid property name")); }
        if decl.fields.insert(key.to_string(), parse_value(raw_value, line_no)?).is_some() {
            return Err(format!("line {line_no}: duplicate property '{key}'"));
        }
    }
    if let Some(decl) = current { return Err(format!("line {}: {} '{}' is missing its closing brace", decl.line, decl.kind.label(), decl.name)); }
    if declarations.is_empty() { return Err("QUR manifest contains no declarations".into()); }
    let mut names = BTreeSet::new();
    for decl in &declarations {
        if !names.insert(decl.name.clone()) { return Err(format!("line {}: duplicate QUR declaration name '{}'", decl.line, decl.name)); }
        validate_fields(decl)?;
    }
    let mut manifest = QurManifest::default();
    for decl in declarations {
        match decl.kind {
            Kind::Chain => {
                let consensus = string_field(&decl, "consensus")?;
                if !["proof_of_stake", "proof_of_authority", "proof_of_work"].contains(&consensus.as_str()) {
                    return Err(format!("line {}: unsupported consensus '{consensus}'", decl.line));
                }
                let native_token = string_field(&decl, "native_token")?;
                if !(2..=12).contains(&native_token.len()) || !native_token.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
                    return Err(format!("line {}: native_token must be a 2-12 character uppercase ticker", decl.line));
                }
                manifest.chains.push(ChainSpec { name: decl.name, consensus, native_token });
            }
            Kind::Token => {
                let standard = string_field(&decl, "standard")?;
                if !["fungible", "non_fungible"].contains(&standard.as_str()) {
                    return Err(format!("line {}: token standard must be 'fungible' or 'non_fungible'", decl.line));
                }
                let (supply_text, quoted) = required(&decl, "supply")?;
                if *quoted { return Err(format!("line {}: token supply must be an unquoted positive integer", decl.line)); }
                let supply: u64 = supply_text.parse().map_err(|_| format!("line {}: invalid token supply", decl.line))?;
                if supply == 0 { return Err(format!("line {}: token supply must be greater than zero", decl.line)); }
                manifest.tokens.push(TokenSpec { name: decl.name, standard, supply });
            }
            Kind::Dao => {
                let governance = string_field(&decl, "governance")?;
                if !["on_chain", "off_chain", "hybrid"].contains(&governance.as_str()) {
                    return Err(format!("line {}: unsupported governance mode '{governance}'", decl.line));
                }
                manifest.daos.push(DaoSpec { name: decl.name, governance });
            }
            Kind::Wallet => {
                let (network, quoted) = required(&decl, "network")?;
                if *quoted || !valid_identifier(network) { return Err(format!("line {}: wallet network must be an unquoted chain identifier", decl.line)); }
                let network = network.clone();
                manifest.wallets.push(WalletSpec { name: decl.name, network });
            }
        }
    }
    if manifest.chains.is_empty() { return Err("QUR manifest must declare at least one chain".into()); }
    for wallet in &manifest.wallets {
        if !manifest.chains.iter().any(|chain| chain.name == wallet.network) {
            return Err(format!("wallet '{}' references unknown chain '{}'", wallet.name, wallet.network));
        }
    }
    Ok(manifest)
}
#[cfg(test)]
mod tests {
    use super::parse;
    const VALID: &str = r#"
chain MyChain {
    consensus = "proof_of_stake"
    native_token = "QOR"
}
token GOLD {
    standard = "fungible"
    supply = 100000000
}
dao Community {
    governance = "on_chain"
}
wallet MyWallet {
    network = MyChain
}
"#;
    #[test] fn parses_the_documented_web3_manifest() {
        let m = parse(VALID).unwrap();
        assert_eq!(m.chains[0].name, "MyChain");
        assert_eq!(m.chains[0].consensus, "proof_of_stake");
        assert_eq!(m.chains[0].native_token, "QOR");
        assert_eq!(m.tokens[0].supply, 100_000_000);
        assert_eq!(m.daos[0].governance, "on_chain");
        assert_eq!(m.wallets[0].network, "MyChain");
    }
    #[test] fn rejects_wallets_that_reference_unknown_chains() {
        let source = r#"chain Main {
consensus = "proof_of_stake"
native_token = "QOR"
}
wallet W {
network = Missing
}
"#;
        assert!(parse(source).unwrap_err().contains("unknown chain"));
    }
    #[test] fn rejects_duplicate_names_and_properties() {
        let duplicate_name = VALID.replace("token GOLD", "token MyChain");
        assert!(parse(&duplicate_name).unwrap_err().contains("duplicate QUR declaration name"));
        let duplicate_property = r#"chain Main {
consensus = "proof_of_stake"
consensus = "proof_of_work"
native_token = "QOR"
}
"#;
        assert!(parse(duplicate_property).unwrap_err().contains("duplicate property"));
    }
    #[test] fn rejects_unknown_properties_and_consensus() {
        let unknown = VALID.replace(r#"native_token = "QOR""#, r#"native_token = "QOR"
magic = 1"#);
        assert!(parse(&unknown).unwrap_err().contains("unknown property"));
        let bad = VALID.replace("proof_of_stake", "instant_finality_magic");
        assert!(parse(&bad).unwrap_err().contains("unsupported consensus"));
    }
    #[test] fn rejects_zero_and_overflowing_token_supply() {
        assert!(parse(&VALID.replace("100000000", "0")).unwrap_err().contains("greater than zero"));
        assert!(parse(&VALID.replace("100000000", "18446744073709551616")).unwrap_err().contains("invalid token supply"));
    }
    #[test] fn rejects_missing_required_fields_and_unclosed_blocks() {
        let missing = r#"chain Main {
consensus = "proof_of_stake"
}
"#;
        assert!(parse(missing).unwrap_err().contains("missing required property 'native_token'"));
        assert!(parse("chain Main {\nconsensus = \"proof_of_stake\"\n").unwrap_err().contains("missing its closing brace"));
    }
    #[test] fn rejects_invalid_identifiers_and_quoted_wallet_networks() {
        assert!(parse("chain 1Bad {\n}\n").unwrap_err().contains("invalid chain name"));
        let quoted = VALID.replace("network = MyChain", "network = \"MyChain\"");
        assert!(parse(&quoted).unwrap_err().contains("unquoted chain identifier"));
    }
    #[test] fn rejects_empty_and_oversized_manifests() {
        assert!(parse("").unwrap_err().contains("no declarations"));
        assert!(parse(&" ".repeat(1024 * 1024 + 1)).unwrap_err().contains("byte safety limit"));
    }
}
