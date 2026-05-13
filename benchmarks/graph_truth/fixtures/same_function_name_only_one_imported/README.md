# Same Function Name Only One Imported

Trap: two modules export chooseUser, but main imports only src/a.ts. A same-name resolver must not link the call to src/b.ts. The adjacent audit and chooseUser calls force exact callsite source-span proof.
