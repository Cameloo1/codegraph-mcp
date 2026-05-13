# Sanitizer Exists But Not On Flow

Trap: sanitizeHtml is imported and nearby, but the user input flow goes raw -> trim -> writeComment without passing through sanitizeHtml.
