import van from "vanjs-core";
import "./music-player.css";
import { type Music } from "./music-list";

const { div, h2, audio } = van.tags;

// const AudioPlayer = {
// 	render(audio: HTMLAudioElement, seekMax: number, currentTime: number) {
// 		return div(
// 			input({
// 				type: "range",
// 				min: 0,
// 				max: seekMax,
// 				value: currentTime,
// 				oninput: e => {
// 					audio.currentTime = Number((e.target as HTMLInputElement).value);
// 				}
// 			})
// 		);
// 	},
// }
// 
// 
// export const MusicPlayer = {
// 	audio: new Audio,
// 	isPlaying: van.state<boolean>(true),
// 	duration: van.state<number>(0),
// 	seekMax: van.state<number>(0),
// 	currentTime: van.state<number>(0),
// 	curPlayingUrl: "",
// 
// 	toggle() {
// 		if(this.isPlaying.val) {
// 			this.audio.pause();
// 			this.isPlaying.val = false;
// 		} else {
// 			this.audio.play();
// 			this.isPlaying.val = true;
// 		}
// 	},
// 	render(curPlaying: Music) {
// 		let iconClassList = ["icon"];
// 		let playIconClassList = ["play-icon"];
// 
// 		if(this.isPlaying.val) {
// 			iconClassList.push("playing");
// 			playIconClassList.push("pause");
// 		} else {
// 			playIconClassList.push("play");
// 		}
// 
// 		return div({ id: "music-player" },
// 			div({ className: iconClassList.join(" "), onclick: () => this.toggle() },
// 				div({ className: playIconClassList.join(" ") }),
// 			),
// 			div({ className: "name-player" },
// 				h2(curPlaying.name),
// 				AudioPlayer.render(this.audio, this.seekMax.val, this.currentTime.val)
// 			)
// 		)
// 	},
// 	loadTrack(url: string) {
// 		if(url === this.curPlayingUrl) return;
// 		this.curPlayingUrl = url;
// 		console.log("loadTrack");
// 
// 		if (this.audio) {
// 			this.audio.pause();
// 			this.audio.src = "";
// 		}
// 
// 		const audio = new Audio(url);
// 		audio.preload = "metadata";
// 		audio.play();
// 
// 		this.audio = audio;
// 
// 		audio.addEventListener("loadedmetadata", () => {
// 			this.duration.val = audio.duration;
// 			this.seekMax.val = audio.duration;
// 		});
// 
// 		audio.addEventListener("timeupdate", () => {
// 			this.currentTime.val = audio.currentTime;
// 		});
// 	},
// }

export const MusicPlayer = {
	render(curPlaying: Music) {
		return div({ id: "music-player" },
			div({ className: "name-player" },
				h2(curPlaying.name),
				audio({ src: `http://192.168.0.107:8000/stream/music/${curPlaying.id}`, controls: true, autoplay: true })
			)
		)
	},
}
