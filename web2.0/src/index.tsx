/* @refresh reload */
import {hydrate, render} from "solid-js/web";
import App from "./App";
import "./index.css";
import {Router, Route} from "@solidjs/router";
import Upload from "./screens/Upload";
import Task from "./screens/Task";

const root = document.getElementById("root");

const appFn = () => (
	<Router root={App}>
		<Route path="/" component={Upload} />
		<Route path="/task/:id" component={Task} />
	</Router>
);

render(appFn, root!);