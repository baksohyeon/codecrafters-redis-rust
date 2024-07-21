
test:
	codecrafters test

submit:
	git stash && git checkout master && git pull && git stash apply && \
	codecrafters submit

run:
	./your_program.sh


kill:
	kill -9 $(shell lsof -t -i:6379)



ping: 
	echo -ne '*1\r\n$4\r\nping\r\n' | nc localhost 6379 \
	