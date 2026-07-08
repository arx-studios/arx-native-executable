use crate::ast::Type;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Rc<RefCell<Vec<Value>>>),
    Void,
}

pub fn default_value(ty: &Type) -> Value {
    match ty {
        Type::Int => Value::Int(0),
        Type::Float => Value::Float(0.0),
        Type::Bool => Value::Bool(false),
        Type::Str => Value::Str(String::new()),
        Type::Array(_) => Value::Array(Rc::new(RefCell::new(Vec::new()))),
        Type::Void => Value::Void,
    }
}

pub fn format_value(v: &Value) -> String {
    match v {
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.clone(),
        Value::Array(_) => "[array]".to_string(),
        Value::Void => String::new(),
    }
}

pub fn as_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        _ => unreachable!("sema guarantees a bool operand here"),
    }
}

pub fn as_int(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        _ => unreachable!("sema guarantees an int operand here"),
    }
}

/// Applies a numeric binary op, dispatching to the int or float arm based on
/// the (sema-guaranteed matching) operand types.
pub fn numeric_op(
    l: Value,
    r: Value,
    int_op: impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
) -> Value {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Value::Int(int_op(a, b)),
        (Value::Float(a), Value::Float(b)) => Value::Float(float_op(a, b)),
        _ => unreachable!("sema guarantees matching numeric operand types"),
    }
}

pub fn numeric_cmp(l: &Value, r: &Value) -> Ordering {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        _ => unreachable!("sema guarantees matching numeric operand types"),
    }
}
