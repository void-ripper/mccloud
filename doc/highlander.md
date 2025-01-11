# Intro

mccloud is a decentrelized data server wich uses the highlander algorithm instead of Proof of Work.

# Highlander Algorithm

The algorithm is called **Highlander**, because like in the movie of the same name, there can only be one.

The algorithm solves the problem to determin in a distributed network of nodes, the one node which
is allowed to create the next valid data set into the distributed database.

An example for this would be the Bitcoin network.
But unlike the PoW (Proof of Work) algorithm, which is used in the Bitcoin network, the Highlander algorithm
does not require to spend a high amount of CPU time and electricity to solve an artificial problem and is not
vulnerable to an 51% attack.

## How the Highlander Algorithm works

All nodes in the network play rock-paper-scissor against each other to determin the winner.

1. Every node has a private and public key.
2. Every new node in the network introduces itself, so every node knows of every other node in the network.
3. Every node can now create for itself a binary tree of the public keys of each node.
4. Every node knows now how many games have to be played to win (log2 nodes) and generates all
    rock-paper-scissor choices to share them in the network.
5. Now every node knows the match up and the choices of every node, so every node in the network can
    independently determin the winner.
6. The winner creates the new data entry and attaches the game which lead to the winning of the node, after that the node signs and shares the data in the network.
7. repeat at step 3.

This behavior leads to a self synchronizing network, which randomly determins one node to be the **choosen one**
to perform an exclusiv action, like creating a new block for a blockchain.
No mining is required. No vast amount of electricity has to be used.
And new blocks can be written relativly fast in seconds.

### Possible Attacks

#### A node just declares itself victor
This does not work, because all other nodes know that the fake winner is not the winner and simply reject
the fake data block.

#### A user spawns over 51% new fake nodes
This would produce vastly inconcistent game matches. If the game which is included in the new block does not
match up the game plan create on the local node, the block is resisted.

#### Delayed Game
The node waits until all other nodes in his game path have send there choices,
to calculate the perfect choices to win.
And with this said, we have already the solution, if the last node which send its choices, wins,
then we reject the game, because it is possibly fraudolent and just start a new one.

### A legit winner does not send its Block
 TODO
