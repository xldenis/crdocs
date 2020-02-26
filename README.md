# CRDOCS

A distributed CRDT based P2P web document editor à la Google Docs but more homegrown ;)

## Running locally

1. Clone
2. Build the backend
 ```
 ( cd backend && cargo build --release )
```
3. Build the front end
```
( cd frontend && wasm-pack build && cd www && yarn build --mode=production )
```
4. Run the backend
```
cd backend && cargo run --release
```
5. Load `localhost:3012`

