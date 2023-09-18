query = {
    "must":
    # queries => map(pluck("fragment")) => flatten()
        [
            clause
            for kf_pair in queries
            for clause in kf_pair["fragment"]
        ],

}

{
    x:
    # comment
    y
    for x in z
}

