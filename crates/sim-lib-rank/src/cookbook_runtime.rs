//! Runtime callables for deterministic rank cookbook recipes.

use std::sync::Arc;

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, Error, Export, Linker, LoadCx, Object,
    ObjectCompat, Result, Symbol, Value,
};

use crate::{
    cookbook::{rank_retrieve_demo, recommendation_ranking_demo, space_coordinate_demo},
    lisp_class::class_value_or_stub,
};

#[derive(Clone)]
struct RankCookbookFunction {
    kind: RankCookbookFunctionKind,
}

#[derive(Clone, Copy)]
enum RankCookbookFunctionKind {
    SpaceCoordinate,
    Retrieve,
    RecommendationRanking,
}

impl RankCookbookFunctionKind {
    fn all() -> [Self; 3] {
        [
            Self::SpaceCoordinate,
            Self::Retrieve,
            Self::RecommendationRanking,
        ]
    }

    fn symbol(self) -> Symbol {
        match self {
            Self::SpaceCoordinate => Symbol::qualified("rank", "space-coordinate-demo"),
            Self::Retrieve => Symbol::qualified("rank", "retrieve-demo"),
            Self::RecommendationRanking => Symbol::qualified("rank", "recommendation-ranking-demo"),
        }
    }
}

impl Object for RankCookbookFunction {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<function {}>", self.kind.symbol()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for RankCookbookFunction {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(
            cx,
            CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("core", "Function"),
        )
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for RankCookbookFunction {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        if !args.values().is_empty() {
            return Err(Error::Eval(format!(
                "{} expects no arguments, got {}",
                self.kind.symbol(),
                args.values().len()
            )));
        }

        let expr = match self.kind {
            RankCookbookFunctionKind::SpaceCoordinate => space_coordinate_demo(),
            RankCookbookFunctionKind::Retrieve => rank_retrieve_demo(),
            RankCookbookFunctionKind::RecommendationRanking => recommendation_ranking_demo(),
        };
        cx.factory().expr(expr)
    }
}

pub(crate) fn install_rank_cookbook_functions(
    cx: &mut LoadCx,
    linker: &mut Linker<'_>,
) -> Result<()> {
    for kind in RankCookbookFunctionKind::all() {
        let function = RankCookbookFunction { kind };
        linker.function_value(
            function.kind.symbol(),
            cx.factory().opaque(Arc::new(function))?,
        )?;
    }
    Ok(())
}

pub(crate) fn rank_cookbook_exports() -> Vec<Export> {
    RankCookbookFunctionKind::all()
        .into_iter()
        .map(|kind| Export::Function {
            symbol: kind.symbol(),
            function_id: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, EagerPolicy, Expr};

    use crate::install_rank_lib;

    use super::*;

    #[test]
    fn rank_cookbook_callables_return_recipe_expressions() {
        let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
        install_rank_lib(&mut cx).unwrap();

        let value = cx
            .eval_expr(Expr::Call {
                operator: Box::new(Expr::Symbol(Symbol::qualified(
                    "rank",
                    "recommendation-ranking-demo",
                ))),
                args: Vec::new(),
            })
            .unwrap();
        let Expr::List(items) = value.object().as_expr(&mut cx).unwrap() else {
            panic!("cookbook function returns a list expression")
        };

        assert!(
            matches!(&items[0], Expr::Symbol(symbol) if symbol.name.as_ref() == "recommendation-ranking")
        );
    }
}
