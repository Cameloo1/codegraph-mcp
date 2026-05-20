package rf
func source() int { return 42 }
func caller() int { value := source(); return value }
