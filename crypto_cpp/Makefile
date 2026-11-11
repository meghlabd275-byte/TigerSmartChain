# TigerSmartChain cryptographic primitives (C++17).
#
# Produces a static library consumed by the Rust `tiger-crypto` crate via
# build.rs. Kept deliberately dependency-free: no OpenSSL, no libsecp256k1.

CXX      ?= g++
CXXFLAGS ?= -O3 -fPIC -Wall -Wextra -std=c++17 -fno-exceptions -fno-rtti

TARGET   := libtiger_crypto.a
SRCS     := keccak256.cpp secp256k1.cpp trie.cpp
OBJS     := $(SRCS:.cpp=.o)
HEADERS  := tiger_crypto.h trie.hpp
TEST     := crypto_test
TRIETEST := trie_test

.PHONY: all clean test

all: $(TARGET)

$(TARGET): $(OBJS)
	ar rcs $@ $(OBJS)

%.o: %.cpp $(HEADERS)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(TEST): crypto_test.cpp $(SRCS) $(HEADERS)
	$(CXX) $(CXXFLAGS) -I. $^ -o $@

$(TRIETEST): trie_test.cpp trie.cpp keccak256.cpp $(HEADERS)
	$(CXX) $(CXXFLAGS) -I. $^ -o $@

test: $(TEST) $(TRIETEST)
	./$(TEST)
	./$(TRIETEST)

clean:
	rm -f $(OBJS) $(TARGET) $(TEST) $(TRIETEST)
