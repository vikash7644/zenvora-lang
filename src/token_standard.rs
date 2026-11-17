//! Native fungible-token state primitives for QORVYNT's existing QVM 2.0 executor.
use std::collections::BTreeMap;

const MAX_TOKENS: usize = 10_000;
const MAX_EVENTS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TokenEvent {
    Transfer { token_id: String, from: Option<String>, to: Option<String>, amount: u128 },
    Approval { token_id: String, owner: String, spender: String, amount: u128 },
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenInfo {
    pub name: String, pub symbol: String, pub decimals: u8, pub owner: String, pub total_supply: u128,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenLedger {
    tokens: BTreeMap<String, TokenInfo>,
    balances: BTreeMap<(String, String), u128>,
    allowances: BTreeMap<(String, String, String), u128>,
    events: Vec<TokenEvent>,
}
impl TokenLedger {
    pub fn new() -> Self { Self::default() }
    pub fn token(&self, id: &str) -> Option<&TokenInfo> { self.tokens.get(id) }
    pub fn balance_of(&self, id: &str, account: &str) -> u128 {
        self.balances.get(&(id.into(), account.into())).copied().unwrap_or(0)
    }
    pub fn allowance(&self, id: &str, owner: &str, spender: &str) -> u128 {
        self.allowances.get(&(id.into(), owner.into(), spender.into())).copied().unwrap_or(0)
    }
    pub fn events(&self) -> &[TokenEvent] { &self.events }
    pub fn is_empty(&self) -> bool { self.tokens.is_empty() && self.balances.is_empty() && self.allowances.is_empty() && self.events.is_empty() }
    fn event_room(&self, n: usize) -> Result<(), String> {
        if self.events.len().checked_add(n).is_none_or(|v| v > MAX_EVENTS) { Err("token event history limit reached".into()) } else { Ok(()) }
    }
    fn token_exists(&self, id: &str) -> Result<(), String> {
        valid_id(id)?;
        if self.tokens.contains_key(id) { Ok(()) } else { Err("unknown token ID".into()) }
    }
    pub fn create(&mut self, id: &str, name: &str, symbol: &str, decimals: u8, owner: &str, supply: u128) -> Result<(), String> {
        valid_id(id)?; valid_account(owner)?;
        if name.trim().is_empty() || name.len() > 64 { return Err("invalid token name".into()); }
        if symbol.is_empty() || symbol.len() > 16 || !symbol.bytes().all(|b| b.is_ascii_alphanumeric()) { return Err("invalid token symbol".into()); }
        if decimals > 18 { return Err("token decimals cannot exceed 18".into()); }
        if self.tokens.len() >= MAX_TOKENS { return Err("token registry is full".into()); }
        if self.tokens.contains_key(id) { return Err("token ID is already registered".into()); }
        self.event_room(usize::from(supply > 0))?;
        self.tokens.insert(id.into(), TokenInfo { name: name.into(), symbol: symbol.into(), decimals, owner: owner.into(), total_supply: supply });
        if supply > 0 {
            self.balances.insert((id.into(), owner.into()), supply);
            self.events.push(TokenEvent::Transfer { token_id: id.into(), from: None, to: Some(owner.into()), amount: supply });
        }
        Ok(())
    }
    pub fn transfer(&mut self, id: &str, from: &str, to: &str, amount: u128) -> Result<(), String> {
        self.move_balance(id, from, to, amount)
    }
    pub fn approve(&mut self, id: &str, owner: &str, spender: &str, amount: u128) -> Result<(), String> {
        self.token_exists(id)?; valid_account(owner)?; valid_account(spender)?; self.event_room(1)?;
        self.allowances.insert((id.into(), owner.into(), spender.into()), amount);
        self.events.push(TokenEvent::Approval { token_id: id.into(), owner: owner.into(), spender: spender.into(), amount });
        Ok(())
    }
    pub fn transfer_from(&mut self, id: &str, spender: &str, owner: &str, to: &str, amount: u128) -> Result<(), String> {
        self.token_exists(id)?; valid_account(spender)?;
        let key = (id.into(), owner.into(), spender.into());
        let allowed = self.allowances.get(&key).copied().unwrap_or(0);
        if amount == 0 { return Err("token transfer amount must be positive".into()); }
        if allowed < amount { return Err("token allowance is insufficient".into()); }
        let mut next = self.clone();
        next.move_balance(id, owner, to, amount)?;
        next.allowances.insert(key, allowed - amount);
        next.event_room(1)?;
        next.events.push(TokenEvent::Approval { token_id: id.into(), owner: owner.into(), spender: spender.into(), amount: allowed - amount });
        *self = next; Ok(())
    }
    pub fn mint(&mut self, id: &str, caller: &str, to: &str, amount: u128) -> Result<(), String> {
        self.token_exists(id)?; valid_account(to)?;
        if self.tokens[id].owner != caller { return Err("only token owner may mint".into()); }
        if amount == 0 { return Err("token mint amount must be positive".into()); }
        let supply = self.tokens[id].total_supply.checked_add(amount).ok_or("token total supply overflow")?;
        let balance = self.balance_of(id, to).checked_add(amount).ok_or("token account balance overflow")?;
        self.event_room(1)?;
        self.tokens.get_mut(id).unwrap().total_supply = supply;
        self.balances.insert((id.into(), to.into()), balance);
        self.events.push(TokenEvent::Transfer { token_id: id.into(), from: None, to: Some(to.into()), amount }); Ok(())
    }
    pub fn burn(&mut self, id: &str, holder: &str, amount: u128) -> Result<(), String> {
        self.token_exists(id)?; valid_account(holder)?;
        if amount == 0 { return Err("token burn amount must be positive".into()); }
        let balance = self.balance_of(id, holder);
        if balance < amount { return Err("insufficient token balance".into()); }
        self.event_room(1)?;
        let mut next = self.clone();
        next.tokens.get_mut(id).unwrap().total_supply = next.tokens[id].total_supply.checked_sub(amount).ok_or("token supply underflow")?;
        next.balances.insert((id.into(), holder.into()), balance - amount);
        next.events.push(TokenEvent::Transfer { token_id: id.into(), from: Some(holder.into()), to: None, amount });
        *self = next; Ok(())
    }
    fn move_balance(&mut self, id: &str, from: &str, to: &str, amount: u128) -> Result<(), String> {
        self.token_exists(id)?; valid_account(from)?; valid_account(to)?;
        if amount == 0 { return Err("token transfer amount must be positive".into()); }
        let balance = self.balance_of(id, from);
        if balance < amount { return Err("insufficient token balance".into()); }
        let target = self.balance_of(id, to).checked_add(amount).ok_or("token account balance overflow")?;
        self.event_room(1)?;
        self.balances.insert((id.into(), from.into()), balance - amount);
        self.balances.insert((id.into(), to.into()), target);
        self.events.push(TokenEvent::Transfer { token_id: id.into(), from: Some(from.into()), to: Some(to.into()), amount }); Ok(())
    }
    pub fn encode_snapshot(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 { return Err("token ledger snapshot exceeds size limit".into()); }
        Ok(bytes)
    }
    pub fn decode_snapshot(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 8 * 1024 * 1024 { return Err("token ledger snapshot exceeds size limit".into()); }
        let value: Self = serde_json::from_slice(bytes).map_err(|e| format!("invalid token snapshot: {e}"))?;
        if value.tokens.len() > MAX_TOKENS || value.events.len() > MAX_EVENTS { return Err("token snapshot record limit exceeded".into()); }
        for (id, info) in &value.tokens {
            valid_id(id)?; valid_account(&info.owner)?;
            if info.name.trim().is_empty() || info.name.len() > 64 || info.symbol.is_empty() || info.symbol.len() > 16 || info.decimals > 18 { return Err("invalid token metadata in snapshot".into()); }
        }
        for ((id, account), _) in &value.balances { if !value.tokens.contains_key(id) { return Err("balance references unknown token".into()); } valid_account(account)?; }
        for ((id, owner, spender), _) in &value.allowances { if !value.tokens.contains_key(id) { return Err("allowance references unknown token".into()); } valid_account(owner)?; valid_account(spender)?; }
        if value.encode_snapshot()?.as_slice() != bytes { return Err("token snapshot is not canonical".into()); }
        Ok(value)
    }
}
fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') { Err("invalid token ID".into()) } else { Ok(()) }
}
fn valid_account(v: &str) -> Result<(), String> {
    if v.is_empty() || v.len() > 64 || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { Err("invalid token account".into()) } else { Ok(()) }
}
#[cfg(test)]
mod tests {
    use super::{TokenEvent, TokenLedger};
    #[test] fn erc20_surface_tracks_balances_allowances_and_events() {
        let mut t = TokenLedger::new(); t.create("coin", "QORVYNT Coin", "QVC", 18, "alice", 1000).unwrap();
        t.transfer("coin", "alice", "bob", 100).unwrap(); t.approve("coin", "bob", "carol", 40).unwrap();
        t.transfer_from("coin", "carol", "bob", "dave", 25).unwrap();
        assert_eq!(t.balance_of("coin", "bob"), 75); assert_eq!(t.balance_of("coin", "dave"), 25);
        assert_eq!(t.allowance("coin", "bob", "carol"), 15);
        assert!(t.events().iter().any(|e| matches!(e, TokenEvent::Approval { amount: 15, .. })));
    }
    #[test] fn owner_mint_and_holder_burn_preserve_supply_invariant() {
        let mut t = TokenLedger::new(); t.create("coin", "QORVYNT Coin", "QVC", 8, "alice", 100).unwrap();
        assert!(t.mint("coin", "bob", "bob", 1).is_err()); t.mint("coin", "alice", "bob", 50).unwrap();
        t.burn("coin", "bob", 20).unwrap(); assert_eq!(t.token("coin").unwrap().total_supply, 130);
    }
    #[test] fn token_snapshot_round_trips_and_failed_transfer_is_atomic() {
        let mut t = TokenLedger::new(); t.create("coin", "QORVYNT Coin", "QVC", 18, "alice", u128::MAX).unwrap();
        let before = t.clone(); assert!(t.transfer("coin", "alice", "bob", 1).is_err()); assert_eq!(t, before);
        let bytes = t.encode_snapshot().unwrap(); assert_eq!(TokenLedger::decode_snapshot(&bytes).unwrap(), t);
    }
}
