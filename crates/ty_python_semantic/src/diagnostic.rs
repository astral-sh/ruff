/// Suggest a name from `existing_names` that is similar to `wrong_name`.
pub(crate) fn did_you_mean<S: AsRef<str>, T: AsRef<str>>(
    existing_names: impl Iterator<Item = S>,
    wrong_name: T,
) -> Option<String> {
    if wrong_name.as_ref().len() < 3 {
        return None;
    }

    existing_names
        .filter(|ref id| id.as_ref().len() >= 2)
        .map(|ref id| {
            (
                id.as_ref().to_string(),
                strsim::damerau_levenshtein(
                    &id.as_ref().to_lowercase(),
                    &wrong_name.as_ref().to_lowercase(),
                ),
            )
        })
        .min_by_key(|(_, dist)| *dist)
        // Heuristic to filter out bad matches
        .filter(|(_, dist)| *dist <= 3)
        .map(|(id, _)| id)
}
