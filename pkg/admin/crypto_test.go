package admin

import (
	"encoding/base32"
	"encoding/hex"
	"strings"
	"testing"
	"time"

	"golang.org/x/crypto/argon2"
)

func hexDecode(value string) ([]byte, error) { return hex.DecodeString(value) }

func hexEncode(value []byte) string { return hex.EncodeToString(value) }

// TestPasswordHashingUsesArgon2id verifies that a stored password hash is the
// Argon2id digest of the password with the stored salt, and that the digest is
// unique per salt.
func TestPasswordHashingUsesArgon2id(t *testing.T) {
	salt, err := generateSalt()
	if err != nil {
		t.Fatalf("generateSalt: %v", err)
	}
	if len(salt) != 64 {
		t.Fatalf("salt should be 32 bytes hex encoded (64 chars), got %d", len(salt))
	}

	const password = "correct horse battery staple"

	hash, err := hashPasswordWithSalt(password, salt)
	if err != nil {
		t.Fatalf("hashPasswordWithSalt: %v", err)
	}

	saltBytes, err := hexDecode(salt)
	if err != nil {
		t.Fatalf("decode salt: %v", err)
	}

	expected := argon2.IDKey([]byte(password), saltBytes, argon2Time, argon2Memory, argon2Threads, argon2KeyLen)
	if hash != hexEncode(expected) {
		t.Fatal("stored hash is not the Argon2id digest of the password")
	}

	// A different salt must yield a different digest for the same password.
	otherSalt, err := generateSalt()
	if err != nil {
		t.Fatalf("generateSalt: %v", err)
	}
	otherHash, err := hashPasswordWithSalt(password, otherSalt)
	if err != nil {
		t.Fatalf("hashPasswordWithSalt: %v", err)
	}
	if otherHash == hash {
		t.Fatal("hashes must differ when the salt differs")
	}

	// A wrong password must not reproduce the digest.
	wrong, err := hashPasswordWithSalt("wrong password", salt)
	if err != nil {
		t.Fatalf("hashPasswordWithSalt: %v", err)
	}
	if wrong == hash {
		t.Fatal("a wrong password produced the stored hash")
	}
}

// TestHashPasswordRejectsWeakSalt ensures a short or non-hex salt is refused
// rather than silently producing a weak hash.
func TestHashPasswordRejectsWeakSalt(t *testing.T) {
	if _, err := hashPasswordWithSalt("pw", "abcd"); err == nil {
		t.Fatal("expected an error for a salt shorter than 16 bytes")
	}
	if _, err := hashPasswordWithSalt("pw", strings.Repeat("zz", 32)); err == nil {
		t.Fatal("expected an error for a non-hex salt")
	}
}

// TestVerificationCodeShape checks the code is six digits.
func TestVerificationCodeShape(t *testing.T) {
	for i := 0; i < 32; i++ {
		code, err := generateVerificationCode()
		if err != nil {
			t.Fatalf("generateVerificationCode: %v", err)
		}
		if len(code) != 6 {
			t.Fatalf("code %q is not six digits", code)
		}
		for _, r := range code {
			if r < '0' || r > '9' {
				t.Fatalf("code %q contains a non-digit", code)
			}
		}
	}
}

// TestTOTPMatchesRFC6238Vector reproduces the published RFC 6238 test vector
// for SHA-1 with an eight digit code truncated to six, confirming the
// implementation follows the standard rather than comparing random values.
func TestTOTPMatchesRFC6238Vector(t *testing.T) {
	// The RFC 6238 SHA-1 seed is the ASCII string "12345678901234567890".
	const secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"

	key, err := base32.StdEncoding.WithPadding(base32.NoPadding).DecodeString(secret)
	if err != nil {
		t.Fatalf("decode seed: %v", err)
	}
	if string(key) != "12345678901234567890" {
		t.Fatalf("unexpected seed %q", string(key))
	}

	// At T = 59 the RFC 6238 eight digit code is 94287082, so the six digit
	// truncation is 287082.
	code, err := hotp(key, 59/totpPeriod)
	if err != nil {
		t.Fatalf("hotp: %v", err)
	}
	if code != "287082" {
		t.Fatalf("hotp at counter 1 = %q, want 287082", code)
	}

	// At T = 1111111109 the RFC code is 07081804, truncated to 081804.
	code, err = hotp(key, 1111111109/totpPeriod)
	if err != nil {
		t.Fatalf("hotp: %v", err)
	}
	if code != "081804" {
		t.Fatalf("hotp at counter 37037036 = %q, want 081804", code)
	}
}

// TestVerifyTOTPRejectsInvalidInput ensures the verifier fails closed.
func TestVerifyTOTPRejectsInvalidInput(t *testing.T) {
	const secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"

	if verifyTOTP(secret, "12345") {
		t.Fatal("a five digit code must be rejected")
	}
	if verifyTOTP(secret, "abcdef") {
		t.Fatal("a non-numeric code must be rejected")
	}
	if verifyTOTP("not-base32!!", "123456") {
		t.Fatal("an undecodable secret must be rejected")
	}

	// A code for the current window must be accepted.
	current, err := hotp(mustDecodeBase32(t, secret), uint64(time.Now().Unix()/totpPeriod))
	if err != nil {
		t.Fatalf("hotp: %v", err)
	}
	if !verifyTOTP(secret, current) {
		t.Fatal("the current window code must be accepted")
	}
}

func mustDecodeBase32(t *testing.T, secret string) []byte {
	t.Helper()
	key, err := base32.StdEncoding.WithPadding(base32.NoPadding).DecodeString(secret)
	if err != nil {
		t.Fatalf("decode secret: %v", err)
	}
	return key
}
