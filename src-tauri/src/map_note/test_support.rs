pub(super) fn body(block: &str, note: &str) -> String {
    format!("{note}\n\n<!-- oc2-map\n{block}\n-->\n")
}
