"""Tests for the RUF062 rule (large numeric literals without underscore separators)."""

# These should trigger the rule (large numbers without underscore separators)
i = 1000000
f = 123456789.123456789
x = 0x1234ABCD
b = 0b10101010101010101010101
o = 0o12345671234

# Scientific notation
sci = 1000000e10
sci_uppercase = 1000000E10

# These should not trigger the rule (small numbers or already have separators)
dec_small_int = 1234
dec_small_float = 123.45
dec_with_separators = 1_000_000
hex_with_separators = 0x1234_ABCD
bin_with_separators = 0b10101_01010101_01010101
oct_with_separators = 0o123_4567_1234
sci_with_separators = 1_000_000e10

# These should trigger the rule because their separators are misplaced
dec_misplaced_separators = 123_4567_89
oct_misplaced_separators = 0o12_34_56
hex_misplaced_separators = 0xABCD_EF
flt_misplaced_separators = 123.12_3456_789

# uppercase base prefix
hex_uppercase = 0XABCDEF
oct_uppercase = 0O123456
bin_uppercase = 0B01010101010101

# Negative numbers should also be checked
neg_large = -1000000
neg_with_separators = -1_000_000 # should not trigger
neg_with_spaces = -   100000
neg_oct = -0o1234567
neg_hex = -0xABCDEF
neg_bin -0b0101010100101
neg_hex_with_spaces = -   0xABCDEF

# Testing for minimun size thresholds
dec_4_digits = 1234  # Should not trigger, just below the threshold of 5 digits
dec_5_digits = 12345  # Should trigger, 5 digits
oct_4_digits = 0o1234  # Should not trigger, just below the threshold of 4 digits
oct_5_digits = 0o12345  # Should trigger, 5 digits
bin_8_digits = 0b01010101  # Should not trigger, just below the threshold of 9 digits
bin_9_digits = 0b101010101  # Should trigger, 9 digits
hex_4_digits = 0xABCD  # Should not trigger, just below the threshold of 5 digits
hex_5_digits = 0xABCDE  # Should trigger, 5 digits
flt_4_digits = .1234  # Should not trigger, just below the threshold of 5 digits
flt_5_digits = .12345  # Should trigger, 5 digits