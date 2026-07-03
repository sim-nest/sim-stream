use std::marker::PhantomData;

use sim_kernel::Symbol;

use crate::{
    Nat, RankBuilder, RankChild, RankCodec, RankDescribe, RankDescribeContext, RankEnumDescriptor,
    RankError, RankGrammar, RankGroupCodec, RankNode, RankRecursive,
};

#[derive(Clone, Copy)]
enum UserMode {
    Off,
    On,
    Auto,
}

impl UserMode {
    fn descriptor() -> RankEnumDescriptor {
        RankEnumDescriptor::new(Self::rank_type_symbol())
            .variant(Symbol::new("off"))
            .variant(Symbol::new("on"))
            .variant(Symbol::new("auto"))
    }

    fn rank_node(self) -> RankNode {
        let variant = match self {
            Self::Off => Symbol::new("off"),
            Self::On => Symbol::new("on"),
            Self::Auto => Symbol::new("auto"),
        };
        Self::descriptor().variant_node(&variant).unwrap()
    }
}

impl RankDescribe for UserMode {
    fn rank_type_symbol() -> Symbol {
        Symbol::qualified("rank-test", "user-mode")
    }

    fn rank_grammar(_cx: &RankDescribeContext) -> crate::RankResult<RankGrammar> {
        Self::descriptor().build()
    }
}

struct Payload;

impl RankDescribe for Payload {
    fn rank_type_symbol() -> Symbol {
        Symbol::qualified("rank-test", "payload")
    }

    fn rank_grammar(_cx: &RankDescribeContext) -> crate::RankResult<RankGrammar> {
        Ok(RankBuilder::bool())
    }
}

struct BinaryTree<T>(PhantomData<T>);

impl<T: RankDescribe> RankDescribe for BinaryTree<T> {
    fn rank_type_symbol() -> Symbol {
        crate::derived_symbol("binary-tree", &[T::rank_type_symbol()])
    }

    fn rank_grammar(cx: &RankDescribeContext) -> crate::RankResult<RankGrammar> {
        let tree = Self::rank_type_symbol();
        let leaf = RankBuilder::product(Symbol::new("leaf"))
            .field(
                Symbol::new("value"),
                RankChild::<T>::rank_grammar(
                    &cx.child(Symbol::new("leaf")).child(Symbol::new("value")),
                )?,
            )
            .build()?;
        let branch = RankBuilder::product(Symbol::new("branch"))
            .field(
                Symbol::new("left"),
                <Box<RankRecursive<Self>> as RankDescribe>::rank_grammar(
                    &cx.child(Symbol::new("branch")).child(Symbol::new("left")),
                )?,
            )
            .field(
                Symbol::new("right"),
                <Box<RankRecursive<Self>> as RankDescribe>::rank_grammar(
                    &cx.child(Symbol::new("branch")).child(Symbol::new("right")),
                )?,
            )
            .build()?;

        RankBuilder::sum(tree)
            .alt(Symbol::new("leaf"), leaf)
            .alt_with_cost(Symbol::new("branch"), 1, branch)
            .build_recursive()
    }
}

#[test]
fn user_enum_needs_no_custom_ordinal_arithmetic() {
    let grammar = UserMode::rank_grammar(&RankDescribeContext::new()).unwrap();
    let codec = RankGroupCodec::new(grammar);
    let node = UserMode::Auto.rank_node();

    let ordinal = codec.rank_node(&node).unwrap();

    assert_eq!(ordinal, Nat::from(2_u64));
    assert_eq!(codec.unrank_node(&ordinal).unwrap(), node);
    assert_eq!(
        codec.unrank_node(&Nat::from(0_u64)).unwrap(),
        UserMode::Off.rank_node()
    );
    assert_eq!(
        codec.unrank_node(&Nat::from(1_u64)).unwrap(),
        UserMode::On.rank_node()
    );
}

#[test]
fn option_vec_tuple_and_box_adapters_build_rankable_grammar() {
    type Fixture = (Option<bool>, Vec<Nat>, Box<(bool, Nat)>);

    let grammar = Fixture::rank_grammar(&RankDescribeContext::new()).unwrap();
    let codec = RankGroupCodec::new(grammar);
    let node = RankNode::Product(vec![
        RankNode::sum(1, RankNode::Bool(true)),
        RankNode::List(vec![RankNode::Nat(Nat::zero()), RankNode::Nat(Nat::one())]),
        RankNode::Product(vec![RankNode::Bool(false), RankNode::Nat(Nat::from(2_u64))]),
    ]);

    let ordinal = codec.rank_node(&node).unwrap();

    assert_eq!(codec.unrank_node(&ordinal).unwrap(), node);
}

#[test]
fn binary_tree_with_ranked_payload_uses_generic_recursion() {
    let ctx = RankDescribeContext::new().with_child_space(Payload::rank_type_symbol());
    let grammar = BinaryTree::<Payload>::rank_grammar(&ctx).unwrap();
    let codec = RankGroupCodec::new(grammar);
    let payload_ref = || RankNode::Ref {
        space: Payload::rank_type_symbol(),
        ordinal: Nat::zero(),
    };
    let leaf = || RankNode::sum(0, RankNode::Product(vec![payload_ref()]));
    let tree = RankNode::sum(1, RankNode::Product(vec![leaf(), leaf()]));

    let ordinal = codec.rank_node(&tree).unwrap();

    assert_eq!(codec.unrank_node(&ordinal).unwrap(), tree);
}

#[test]
fn missing_child_space_errors_name_the_field_path() {
    let err = BinaryTree::<Payload>::rank_grammar(&RankDescribeContext::new()).unwrap_err();

    assert_eq!(
        err,
        RankError::MissingChildSpace {
            path: "leaf.value".to_owned(),
            id: Payload::rank_type_symbol()
        }
    );
    assert!(
        err.to_string().contains("leaf.value"),
        "missing child-space diagnostic must include the field path"
    );
}
