#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
enum Token {}

#[salsa::tracked]
struct TokenTree<'db> {
    #[returns(ref)]
    tokens: Vec<Token>,
}
