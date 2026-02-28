import van from "vanjs-core";
import "./music-player.css";
import { MusicList } from "./music-list";

const { div, h2 } = van.tags;

export const MusicPlayer = {
	isPlaying: van.state<boolean>(false),
	render() {
		let iconClassList = ["icon"];
		return div({ id: "music-player" },
			div({ className: iconClassList.join(" ") },
				div({ className: "play-icon" }),
			),
			div({ className: "name-player" },
				h2(MusicList.getCurrentPlaying()!.name)
			)
		)
	},
	mount() {
		return this.render();
	}
}
