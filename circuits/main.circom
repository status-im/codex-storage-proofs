pragma circom 2.1.0;

include "./storer.circom";

component main { public [root, salt] } = StorageProver(32, 4, 2);
