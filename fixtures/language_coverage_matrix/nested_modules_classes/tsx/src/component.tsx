export class Panel {
  renderTitle() { return <h1>Title</h1>; }
  render() { return <section>{this.renderTitle()}</section>; }
}
export function App() { return <Panel />; }
