def real():
    return 1

def run(provider):
    provider.real = lambda: 2
    return provider.real()
