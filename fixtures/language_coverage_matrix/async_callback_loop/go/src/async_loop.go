package async
func worker(value int) {}
func run(items []int) { for _, item := range items { go worker(item) } }
