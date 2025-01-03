use serde::Deserialize;
use tagged_serde::TaggedSerde;

// #[derive(TaggedSerde, Clone, Serialize)]
// #[serde(into = "OpDerived")]
// enum Op {
//     #[tagged_serde = 5]
//     SetOptions { foo: u64 },
// }

// #[derive(Serialize)]
// enum OpCopy {
//     SetOptions { foo: u64 },
// }

// #[derive(Serialize)]
// struct OpDerived {
//     tag: u64,
//     body: OpCopy,
// }

// impl From<Op> for OpDerived {}

// impl TryFrom<OpDerived> for Op {}

#[derive(TaggedSerde, PartialEq, Debug)]
enum Op {
    #[tagged_serde = 5]
    SetOptions(u64, u64),
    #[tagged_serde = 42]
    GetOptions(bool),
}

/*
impl Serialize for Op {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Op::SetOptions { foo } => (5, OpCopy::SetOptions { *foo }).serialize(serializer),
        }
    }
}
*/

fn main() {
    let op = Op::SetOptions(12, 13);

    let json = serde_json::to_string(&op).unwrap();

    assert_eq!(json, "[5,12,13]");

    let new_op: Op = serde_json::from_str(&json).unwrap();
    assert_eq!(new_op, op);
}
