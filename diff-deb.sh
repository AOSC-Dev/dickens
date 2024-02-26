#!/bin/sh
# Usage: diff-deb.sh left.deb right.deb
left=a
dpkg --contents $1 | awk '!($2=$3=$4=$5="")' > $left
touch -m -d "1980-01-01" $left

right=b
dpkg --contents $2 | awk '!($2=$3=$4=$5="")' > $right
touch -m -d "1980-01-01" $right

diff -u $left $right
