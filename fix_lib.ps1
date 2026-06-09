$content = Get-Content crates/crabide-ui/src/lib.rs -Raw

# Fix EditorGroup reference - use crate::state::EditorGroup
$content = $content -replace '(?<!\w)EditorGroup(?!\w)', 'crate::state::EditorGroup'

Set-Content crates/crabide-ui/src/lib.rs $content
