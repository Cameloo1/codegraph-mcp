package same

func duplicate(value int) int { return value + 1 }

type Box struct{}
func (Box) duplicate() int { return 2 }

func caller() int { return duplicate(1) }
