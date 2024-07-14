from entab import Reader


class SpectraReader:
    """
    A Reader that returns a sequence of spectra instead of the individual points.

    This takes the same arguments as a `Reader` (data, filename, parser) in addition to
    `time_res` which controls how points are bucketed by time and `merge_fn` which is
    used to e.g. sum together the points with the same mz/wavelength in the same time
    bucket.
    """
    def __init__(self, data=None, filename=None, parser=None, time_res=0, merge_fn=sum):
        if not isinstance(data, Reader):
            reader = Reader(data, filename=filename, parser=parser)
        else:
            reader = data
        if reader is None:
            raise ValueError('A reader, data, or a filename is required')
        
        if 'mz' in reader.headers:
            y_key = 'mz'
        elif 'wavelength' in reader.headers:
            y_key = 'wavelength'
        else:
            raise KeyError(f'{reader.parser} reader missing mz/wavelength?')
        point = next(reader)
        self._reader = reader
        self._t_key = 'time'
        self._y_key = y_key
        self._z_key = 'intensity'
        self._time_res = time_res
        self._merge_fn = merge_fn
        self._cur_time = getattr(point, self._t_key)
        self._cur_spectra = {getattr(point, self._y_key): [getattr(point, self._z_key)]}

    def __getattr__(self, name):
        return getattr(self._reader, name)

    def __iter__(self):
        return self

    def __next__(self):
        for point in self._reader:
            if getattr(point, self._t_key) - self._cur_time > self._time_res:
                time = self._cur_time
                spectra = {y: self._merge_fn(z) for y, z in self._cur_spectra.items()}
                self._cur_spectra = {}
                self._cur_time = getattr(point, self._t_key)
                return (time, spectra)
            if getattr(point, self._y_key) in self._cur_spectra:
                self._cur_spectra[getattr(point, self._y_key)].append(getattr(point, self._z_key))
            else:
                self._cur_spectra[getattr(point, self._y_key)] = [getattr(point, self._z_key)]

        if len(self._cur_spectra) == 0:
            raise StopIteration()
        spectra = {y: self._merge_fn(z) for y, z in self._cur_spectra.items()}
        self._cur_spectra = {}
        return (self._cur_time, spectra)

