// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use common_expression::types::NumberDataType;

pub enum OP {
    Plus,
    Minus,
    Mul,
    Div,
    IntDiv,
    Modulo,

    Super,
}

pub fn codegen_arithmetic_type_v2() {
    let dest = Path::new("src/query/expression/src/types");
    let path = dest.join("arithmetics_type.rs");

    let mut file = File::create(&path).expect("open");
    let codegen_src_path = file!();
    // Write the head.
    writeln!(
        file,
        "// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the \"License\");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an \"AS IS\" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// This code is generated by {codegen_src_path}. DO NOT EDIT.

use ordered_float::OrderedFloat;

use super::number::Number;

pub trait ResultTypeOfBinary: Sized {{
    type AddMul: Number;
    type Minus: Number;
    type IntDiv: Number;
    type Modulo: Number;
    type LeastSuper: Number;
}}

pub trait ResultTypeOfUnary: Sized {{
    type Negate: Number;

    fn checked_add(self, _rhs: Self) -> Option<Self>;

    fn checked_sub(self, _rhs: Self) -> Option<Self>;

    fn checked_mul(self, _rhs: Self) -> Option<Self>;

    fn checked_div(self, _rhs: Self) -> Option<Self>;

    fn checked_rem(self, _rhs: Self) -> Option<Self>;
}}"
    )
    .unwrap();

    let number_types = vec![
        NumberDataType::UInt8,
        NumberDataType::UInt16,
        NumberDataType::UInt32,
        NumberDataType::UInt64,
        NumberDataType::Int8,
        NumberDataType::Int16,
        NumberDataType::Int32,
        NumberDataType::Int64,
        NumberDataType::Float32,
        NumberDataType::Float64,
    ];

    for lhs in &number_types {
        for rhs in &number_types {
            let add_mul = arithmetic_coercion(*lhs, *rhs, OP::Plus);
            let minus = arithmetic_coercion(*lhs, *rhs, OP::Minus);
            let intdiv = arithmetic_coercion(*lhs, *rhs, OP::IntDiv);
            let modulo = arithmetic_coercion(*lhs, *rhs, OP::Modulo);
            let least_super = arithmetic_coercion(*lhs, *rhs, OP::Super);

            writeln!(
                file,
                "
impl ResultTypeOfBinary for ({}, {}) {{
    type AddMul = {};
    type Minus = {};
    type IntDiv = {};
    type Modulo = {};
    type LeastSuper = {};
}}",
                to_primitive_str(lhs.clone()),
                to_primitive_str(rhs.clone()),
                to_primitive_str(add_mul),
                to_primitive_str(minus),
                to_primitive_str(intdiv),
                to_primitive_str(modulo),
                to_primitive_str(least_super),
            )
            .unwrap();
        }
    }

    for arg in &number_types {
        let negate = neg_coercion(*arg);

        match negate {
            NumberDataType::Float32 | NumberDataType::Float64 => {
                writeln!(
                    file,
                    "
impl ResultTypeOfUnary for {} {{
    type Negate = {};

    fn checked_add(self, rhs: Self) -> Option<Self> {{
        Some(self + rhs)
    }}

    fn checked_sub(self, rhs: Self) -> Option<Self> {{
        Some(self - rhs)
    }}

    fn checked_mul(self, rhs: Self) -> Option<Self> {{
        Some(self * rhs)
    }}

    fn checked_div(self, rhs: Self) -> Option<Self> {{
        Some(self / rhs)
    }}

    fn checked_rem(self, rhs: Self) -> Option<Self> {{
        Some(self % rhs)
    }}
}}",
                    to_primitive_str(arg.clone()),
                    to_primitive_str(negate),
                )
                .unwrap();
            }

            _ => {
                writeln!(
                    file,
                    "
impl ResultTypeOfUnary for {} {{
    type Negate = {};

    fn checked_add(self, rhs: Self) -> Option<Self> {{
        self.checked_add(rhs)
    }}

    fn checked_sub(self, rhs: Self) -> Option<Self> {{
        self.checked_sub(rhs)
    }}

    fn checked_mul(self, rhs: Self) -> Option<Self> {{
        self.checked_mul(rhs)
    }}

    fn checked_div(self, rhs: Self) -> Option<Self> {{
        self.checked_div(rhs)
    }}

    fn checked_rem(self, rhs: Self) -> Option<Self> {{
        self.checked_rem(rhs)
    }}
}}",
                    to_primitive_str(arg.clone()),
                    to_primitive_str(negate),
                )
                .unwrap();
            }
        }
    }
    file.flush().unwrap();
}

fn to_primitive_str(dt: NumberDataType) -> &'static str {
    match dt {
        NumberDataType::UInt8 => "u8",
        NumberDataType::UInt16 => "u16",
        NumberDataType::UInt32 => "u32",
        NumberDataType::UInt64 => "u64",
        NumberDataType::Int8 => "i8",
        NumberDataType::Int16 => "i16",
        NumberDataType::Int32 => "i32",
        NumberDataType::Int64 => "i64",
        NumberDataType::Float32 => "OrderedFloat<f32>",
        NumberDataType::Float64 => "OrderedFloat<f64>",
    }
}

fn arithmetic_coercion(a: NumberDataType, b: NumberDataType, op: OP) -> NumberDataType {
    let is_signed = a.is_signed() || b.is_signed();
    let is_float = a.is_float() || b.is_float();
    let bit_width = a.bit_width().max(b.bit_width());

    match op {
        OP::Plus | OP::Mul => NumberDataType::new(next_bit_width(bit_width), is_signed, is_float),
        OP::Modulo => {
            if is_float {
                return NumberDataType::Float64;
            }
            let result_is_signed = a.is_signed();
            let right_size = b.bit_width();
            let size_of_result = if result_is_signed {
                next_bit_width(right_size)
            } else {
                right_size
            };

            NumberDataType::new(size_of_result, result_is_signed, false)
        }
        OP::Minus => NumberDataType::new(next_bit_width(bit_width), true, is_float),
        OP::Div => NumberDataType::Float64,
        OP::IntDiv => NumberDataType::new(bit_width, is_signed, false),
        OP::Super => NumberDataType::new(bit_width, is_signed, is_float),
    }
}

fn neg_coercion(a: NumberDataType) -> NumberDataType {
    let bit_width = if a.is_signed() {
        a.bit_width()
    } else {
        next_bit_width(a.bit_width())
    };

    NumberDataType::new(bit_width, true, a.is_float())
}

const fn next_bit_width(width: u8) -> u8 {
    if width < 64 { width * 2 } else { 64 }
}
