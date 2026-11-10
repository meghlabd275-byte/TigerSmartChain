# TigerSmartChain cryptographic primitives (C++17).
#
# Produces a static library consumed by the Rust `tiger-crypto` crate via
# build.rs. Kept deliberately dependency-free: no OpenSSL, no libsecp256k1.

CXX      ?= g++
CXXFLAGS ?= -O3 -fPIC -Wall -Wextra -std=c++17 -fno-exceptions -fno-rtti

TARGET := libtiger_crypto.a
SRCS   := keccak256.cpp secp256k1.cpp
OBJS   := $(SRCS:.cpp=.o)
TEST   := crypto_test

.PHONY: all clean test

all: $(TARGET)

$(TARGET): $(OBJS)
	ar rcs $@ $(OBJS)

%.o: %.cpp tiger_crypto.h
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(TEST): crypto_test.cpp $(SRCS) tiger_crypto.h
	$(CXX) $(CXXFLAGS) -I. $^ -o $@

test: $(TEST)
	./$(TEST)

clean:
	rm -f $(OBJS) $(TARGET) $(TEST)
