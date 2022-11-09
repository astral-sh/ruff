use operational_transform::OperationSeq;

pub fn main() {
    let s = "Union[int, Option[str]]";
    let mut a = OperationSeq::default();
    a.delete("Union[".len() as u64);
    a.retain("int".len() as u64);
    a.delete(",".len() as u64);
    a.insert(" |");
    a.retain(" Option[str]".len() as u64);
    a.delete("]".len() as u64);

    let mut b = OperationSeq::default();
    b.retain("Union[int, ".len() as u64);
    b.delete("Option[".len() as u64);
    b.retain("str".len() as u64);
    b.insert(" | None");
    b.delete("]".len() as u64);
    b.retain("]".len() as u64);

    let (_, b_prime) = a.transform(&b).unwrap();
    let ab_prime = a.compose(&b_prime).unwrap();
    let s = ab_prime.apply(s).unwrap();
    println!("{}", s); // int | str | None
}
