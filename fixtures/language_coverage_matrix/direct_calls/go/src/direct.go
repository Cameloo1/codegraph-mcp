package direct
func target(x int) int { return x + 1 }
func caller() int { return target(1) }
