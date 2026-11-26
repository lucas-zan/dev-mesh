import unittest
from validate_live import passed, run_issue, doctor_checks

class LiveValidationTests(unittest.TestCase):
    def test_missing_or_unexecuted_checks_never_pass(self):
        self.assertFalse(passed([]))
        self.assertFalse(passed([{"status":"PASS"},{"status":"NOT_RUN"}]))
        self.assertFalse(passed([{"status":"FAIL"}]))
        self.assertTrue(passed([{"status":"PASS"}]))

    def test_doctor_requires_real_connections_and_native_handshake(self):
        report={"gitea":{"matches":True},"relay":{"authenticated":True},"native_protocols":[{"runtime":"codex","handshake":False,"quiesced":False}]}
        self.assertFalse(passed(doctor_checks(report)))
        report["native_protocols"][0].update(handshake=True,quiesced=True)
        self.assertTrue(passed(doctor_checks(report)))

    def test_real_task_driver_waits_for_delivery_and_records_no_raw_body(self):
        calls=[]
        def execute(args):
            calls.append(args)
            if args[:2]==["issue","get"]:
                state="review" if any(c[:1]==["work"] for c in calls) else "ready"
                return {"state":"open","body":f'private human text <!-- devmesh:issue {{"state":"{state}","delivery":{{"candidate_sha":"abc"}}}} -->'}
            if args[:2]==["worker","status"]:return {"works":[]}
            if args[:1]==["work"]:return {"work_id":"work-1"}
            return {"state":"starting"}
        result=run_issue(execute,128,0,"codex",1)
        self.assertEqual(result["status"],"PASS")
        self.assertEqual(result["candidate_sha"],"abc")
        self.assertNotIn("private human text",str(result))
        self.assertIn(["worker","start"],calls)

    def test_blocked_task_is_failure(self):
        reads=iter(["ready","blocked"])
        def execute(args):
            if args[:2]==["issue","get"]:return {"body":'<!-- devmesh:issue {"state":"'+next(reads)+'"} -->'}
            return {}
        self.assertEqual(run_issue(execute,128,0,"pi",1)["status"],"FAIL")

if __name__=="__main__":unittest.main()
