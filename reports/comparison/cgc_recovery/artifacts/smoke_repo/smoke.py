def cgc_smoke_py_target(value: int) -> int:
    return value + 1


def cgc_smoke_py_caller(input_value: int) -> int:
    return cgc_smoke_py_target(input_value)