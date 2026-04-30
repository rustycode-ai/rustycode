use rustycode_orchestration::{ErrorCategory, ErrorClassifier};

#[test]
fn test_classify_syntax_error() {
    let classifier = ErrorClassifier::new();
    let cat = classifier.classify("bash: syntax error near unexpected token", 2);
    assert_eq!(cat, ErrorCategory::SyntaxError);
}

#[test]
fn test_classify_compile_error() {
    let classifier = ErrorClassifier::new();
    let cat = classifier.classify("error[E0599]: no method named `add`", 101);
    assert_eq!(cat, ErrorCategory::CompileError);
}

#[test]
fn test_classify_by_exit_code_permission_denied() {
    let classifier = ErrorClassifier::new();
    let cat = classifier.classify("some output", 13);
    assert_eq!(cat, ErrorCategory::PermissionDenied);
}

#[test]
fn test_classify_unknown_becomes_custom() {
    let classifier = ErrorClassifier::new();
    let cat = classifier.classify("weird error we haven't seen", 99);
    assert!(matches!(cat, ErrorCategory::Custom(_)));
}
