class Event:
    event_name: ClassVar[str]

    @staticmethod
    def cls_for(event_name: str) -> type[Event]:
        event_cls = _CONCRETE_EVENT_CLASSES.get(event_name)
        if event_cls is not <RANGE_START>None:
            return event_cls<RANGE_END>
        else:
            raise ValueError(f"unknown event name '{event_name}'")
