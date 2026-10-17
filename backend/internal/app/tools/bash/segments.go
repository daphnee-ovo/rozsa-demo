// Package bash implements shell execution and the shared permission segment contract.
package bash

// SplitSegments separates shell command segments at unquoted pipes, logical
// operators, and semicolons. It mirrors the current Rust permission contract.
func SplitSegments(command string) []string {
	segments := make([]string, 0, 1)
	start := 0
	for index := 0; index < len(command); {
		switch command[index] {
		case '\'':
			index++
			for index < len(command) && command[index] != '\'' {
				index++
			}
			index++
		case '"':
			index++
			for index < len(command) {
				if command[index] == '\\' {
					index += 2
				} else if command[index] == '"' {
					index++
					break
				} else {
					index++
				}
			}
		case '|':
			segments = append(segments, trimSegment(command[start:index]))
			if index+1 < len(command) && command[index+1] == '|' {
				index += 2
			} else {
				index++
			}
			start = index
		case '&':
			if index+1 < len(command) && command[index+1] == '&' {
				segments = append(segments, trimSegment(command[start:index]))
				index += 2
				start = index
			} else {
				index++
			}
		case ';':
			segments = append(segments, trimSegment(command[start:index]))
			index++
			start = index
		default:
			index++
		}
	}
	if last := trimSegment(command[start:]); last != "" {
		segments = append(segments, last)
	}
	return segments
}

func trimSegment(segment string) string {
	start, end := 0, len(segment)
	for start < end && (segment[start] == ' ' || segment[start] == '\t' || segment[start] == '\r' || segment[start] == '\n') {
		start++
	}
	for end > start && (segment[end-1] == ' ' || segment[end-1] == '\t' || segment[end-1] == '\r' || segment[end-1] == '\n') {
		end--
	}
	return segment[start:end]
}
