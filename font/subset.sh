#!/bin/sh

pyftsubset "$1" --no-hinting --with-zopfli \
  --text="abcdefghijklmnopqrstuvwxyzåäöABCDEFGHIJKLMNOPQRSTUVWXYZÅÄÖ0123456789,.-;:_!\"'#%&/()[]{}<>|=?+\\*^~"
