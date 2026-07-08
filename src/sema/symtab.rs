use crate::ast::Type;
use std::collections::HashMap;

pub type SymbolId = u32;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: SymbolId,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct FuncSig {
    pub params: Vec<Type>,
    pub return_ty: Type,
}

pub struct SymbolTable {
    scopes: Vec<HashMap<String, Symbol>>,
    functions: HashMap<String, FuncSig>,
    next_symbol_id: SymbolId,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            next_symbol_id: 0,
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn declare_var(&mut self, name: &str, ty: Type) -> SymbolId {
        let id = self.next_symbol_id;
        self.next_symbol_id += 1;
        self.scopes
            .last_mut()
            .expect("at least one scope is always active")
            .insert(name.to_string(), Symbol { id, ty });
        id
    }

    pub fn is_declared_in_current_scope(&self, name: &str) -> bool {
        self.scopes
            .last()
            .expect("at least one scope is always active")
            .contains_key(name)
    }

    pub fn resolve_var(&self, name: &str) -> Option<&Symbol> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    pub fn declare_func(&mut self, name: String, sig: FuncSig) {
        self.functions.insert(name, sig);
    }

    pub fn resolve_func(&self, name: &str) -> Option<&FuncSig> {
        self.functions.get(name)
    }
}
