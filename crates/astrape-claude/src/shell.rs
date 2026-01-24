use astrape_core::HookEvent;

pub fn generate_hook_script(event: HookEvent) -> String {
    let event_name = event.as_str();
    format!(
        r#"#!/bin/bash
# Astrape managed hook - {event_name}
# Do not edit - regenerate with: astrape hook install

INPUT=$(cat)
OUTPUT=$(echo "$INPUT" | astrape hook run {event_name} 2>/dev/null)

if [ $? -eq 0 ] && [ -n "$OUTPUT" ]; then
    echo "$OUTPUT"
else
    echo '{{"continue": true}}'
fi
"#
    )
}

pub fn generate_keyword_detector_script() -> String {
    r#"#!/bin/bash
# Astrape Keyword Detector Hook
# Detects ultrawork/search/analyze keywords and injects enhanced mode messages

INPUT=$(cat)

# Extract the prompt text - try multiple JSON paths
PROMPT=""
if command -v jq &> /dev/null; then
  PROMPT=$(echo "$INPUT" | jq -r '
    if .prompt then .prompt
    elif .message.content then .message.content
    elif .parts then ([.parts[] | select(.type == "text") | .text] | join(" "))
    else ""
    end
  ' 2>/dev/null)
fi

# Fallback: simple grep extraction if jq fails
if [ -z "$PROMPT" ] || [ "$PROMPT" = "null" ]; then
  PROMPT=$(echo "$INPUT" | grep -oP '"(prompt|content|text)"\s*:\s*"\K[^"]+' | head -1)
fi

# Exit if no prompt found
if [ -z "$PROMPT" ]; then
  echo '{"continue": true}'
  exit 0
fi

# Use astrape for keyword detection
OUTPUT=$(echo "$PROMPT" | astrape hook detect-keywords 2>/dev/null)

if [ $? -eq 0 ] && [ -n "$OUTPUT" ]; then
    echo "$OUTPUT"
else
    echo '{"continue": true}'
fi
"#
    .to_string()
}

pub fn generate_stop_continuation_script() -> String {
    r#"#!/bin/bash
# Astrape Stop Continuation Hook
# Enforces todo completion before stopping

INPUT=$(cat)

# Use astrape for todo continuation checking
OUTPUT=$(echo "$INPUT" | astrape hook check-todos 2>/dev/null)

if [ $? -eq 0 ] && [ -n "$OUTPUT" ]; then
    echo "$OUTPUT"
else
    echo '{"continue": true}'
fi
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_hook_script() {
        let script = generate_hook_script(HookEvent::UserPromptSubmit);
        assert!(script.contains("UserPromptSubmit"));
        assert!(script.contains("astrape hook run"));
    }

    #[test]
    fn test_generate_keyword_detector() {
        let script = generate_keyword_detector_script();
        assert!(script.contains("Keyword Detector"));
        assert!(script.contains("detect-keywords"));
    }
}
