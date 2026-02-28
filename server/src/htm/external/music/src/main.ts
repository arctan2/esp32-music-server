import './style.css';
import van from "vanjs-core";
import { MusicList } from './music-list';
import { MusicPlayer } from './music-player';

const app = document.querySelector<HTMLDivElement>('#app')!;

const { div } = van.tags;

const Root = () => {
	return div({ id: "root" },
		div({ id: "music-list", className: "scroll-bar" },
		   MusicList.mount(),
		),
		MusicList.curPlaying.val !== null ? MusicPlayer.mount() : null
	)
}

van.add(app, Root);

MusicList.fetchMusicList();
