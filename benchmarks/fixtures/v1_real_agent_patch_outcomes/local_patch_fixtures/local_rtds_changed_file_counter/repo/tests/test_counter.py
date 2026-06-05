from src.counter import next_counter_value


def test_counter_increment():
    assert next_counter_value(4) == 5
