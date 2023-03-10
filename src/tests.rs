use crate::ignore::{IgnoreEntry, IgnoreFile, IgnoreSelector};

#[test]
fn test_ignore_syntax() {
    let input = r#"
    # This is a comment
    owner:foo
    name:bar
    full_name:baz
    !owner:fizz
    !name:buzz

    !full_name:qux
    "#;
    let expected = IgnoreFile {
        entries: vec![
            IgnoreEntry::new(IgnoreSelector::Owner, "foo"),
            IgnoreEntry::new(IgnoreSelector::Name, "bar"),
            IgnoreEntry::new(IgnoreSelector::FullName, "baz"),
        ],
        inverted_entries: vec![
            IgnoreEntry::new(IgnoreSelector::Owner, "fizz"),
            IgnoreEntry::new(IgnoreSelector::Name, "buzz"),
            IgnoreEntry::new(IgnoreSelector::FullName, "qux"),
        ],
    };

    assert_eq!(input.parse::<IgnoreFile>().unwrap(), expected);
}
