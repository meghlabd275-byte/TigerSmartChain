CC = g++
CFLAGS = -O3 -fPIC -Wall -std=c++17
LDFLAGS = -shared

TARGET = libtiger_crypto.a
SRCS = keccak256.cpp ecdsa.cpp
OBJS = $(SRCS:.cpp=.o)

all: $(TARGET)

$(TARGET): $(OBJS)
	ar rcs $@ $(OBJS)

%.o: %.cpp
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(OBJS) $(TARGET)
