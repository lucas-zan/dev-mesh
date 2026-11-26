import type {Issue} from '../ts/issue';
import type {Wire} from '../ts/wire';
type Role=Issue['required_role'];
const role:Role='backend';
function identity(frame:Wire):string{return frame.frame_id;}
void role;void identity;
