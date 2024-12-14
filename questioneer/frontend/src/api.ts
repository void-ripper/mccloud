
export interface Block {
    hash: string
    data: string[]
}

export interface Peer {
    id: string
    connections: string[]
    all_known: string[]
}
