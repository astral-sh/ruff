"""Test that unicode identifiers are NFKC-normalised"""

𝒞 = 500
print(𝒞)
print(C + 𝒞)  # 2 references to the same variable due to NFKC normalization
print(C / 𝒞)
print(C == 𝑪 == 𝒞 == 𝓒 == 𝕮)

print(𝒟)  # F821
