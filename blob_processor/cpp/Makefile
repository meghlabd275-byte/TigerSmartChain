# TigerSmartChain blob processor.
#
# CMake is the supported build. This Makefile exists so the component can be
# built and tested without CMake, which is useful in minimal containers. Both
# paths build the same sources with the same flags.
#
# Deliberately not -march=native: see the comment in CMakeLists.txt.

CXX      ?= g++
CC       ?= gcc
CXXFLAGS ?= -O3 -fPIC -Wall -Wextra -std=c++20
CFLAGS   ?= -O3 -fPIC -Wall -Wextra

# blst's x86-64 assembly needs BMI2 and ADX. Probing keeps this working on
# toolchains that lack them; the scalar fallback is slower but correct.
BMI2_ADX := $(shell $(CC) -mbmi2 -madx -E -x c /dev/null >/dev/null 2>&1 && echo "-mbmi2 -madx -D__ADX__")

BUILD    := build
BLST     := third_party/blst
CKZG     := third_party/ckzg
CRYPTO   := ../../crypto_cpp

INCLUDES := -I include -I $(BLST)/bindings -I $(CKZG)/src -I $(CRYPTO)

BLST_OBJS  := $(BUILD)/blst_server.o $(BUILD)/blst_assembly.o
CKZG_OBJS  := $(BUILD)/ckzg.o
CRYPTO_OBJS := $(BUILD)/keccak256.o
CORE_OBJS  := $(BUILD)/field_element.o $(BUILD)/kzg.o $(BUILD)/crypto_utils.o \
              $(BUILD)/blob_processor.o $(BUILD)/trusted_setup_data.o

TEST_BIN := $(BUILD)/kzg_vectors_test

.PHONY: all test clean

all: $(BUILD)/libtiger_blob.a

$(BUILD)/libtiger_blob.a: $(BLST_OBJS) $(CKZG_OBJS) $(CRYPTO_OBJS) $(CORE_OBJS)
	ar rcs $@ $^

$(BUILD)/blst_server.o: $(BLST)/src/server.c | $(BUILD)
	$(CC) $(CFLAGS) $(BMI2_ADX) -fno-builtin -I $(BLST)/bindings -c $< -o $@

$(BUILD)/blst_assembly.o: $(BLST)/build/assembly.S | $(BUILD)
	$(CC) $(CFLAGS) $(BMI2_ADX) -I $(BLST)/bindings -c $< -o $@

$(BUILD)/ckzg.o: $(CKZG)/src/ckzg.c | $(BUILD)
	$(CC) $(CFLAGS) -I $(CKZG)/src -I $(BLST)/bindings -c $< -o $@

$(BUILD)/keccak256.o: $(CRYPTO)/keccak256.cpp | $(BUILD)
	$(CXX) $(CXXFLAGS) -I $(CRYPTO) -c $< -o $@

$(BUILD)/%.o: src/%.cpp | $(BUILD)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

$(BUILD)/kzg_vectors.o: tests/kzg_vectors.cpp | $(BUILD)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

$(TEST_BIN): $(BUILD)/kzg_vectors.o $(CORE_OBJS) $(CKZG_OBJS) $(BLST_OBJS) $(CRYPTO_OBJS)
	$(CXX) $(CXXFLAGS) $^ -o $@ -lm

$(BUILD):
	mkdir -p $(BUILD)

test: $(TEST_BIN)
	./$(TEST_BIN)

clean:
	rm -rf $(BUILD)
